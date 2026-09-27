//! Progressive-JPEG DC-only fast decoder for thumbnails.
//!
//! The math that makes this worth a dedicated decoder: every non-DC DCT
//! basis function has zero mean over its 8x8 block, so a 1/8-scale decode
//! (one pixel per block = the block average) depends ONLY on the DC
//! coefficient. A progressive JPEG codes DC in dedicated scans (Ss=Se=0)
//! that typically carry ~10-20% of the entropy bytes; every AC scan
//! (Ss>=1) can be skipped byte-wise without changing the 1/8 output at
//! all. libjpeg-turbo's scaled IDCT (and therefore libvips/ffmpeg
//! shrink-on-load at 1/8) computes exactly this DC-only image, but still
//! pays full entropy decoding for the AC scans — this path skips them.
//!
//! Sample reconstruction, chroma upsampling (fir for 2x factors) and the
//! YCbCr→RGB fixed-point constants mirror `jpeg-decoder` 0.3 so the two
//! decoders produce byte-identical 1/8 output (pinned by tests).
//!
//! Strictness: anything unusual (arithmetic coding, 12-bit,
//! 4-component CMYK/YCCK, 16-bit quant tables, malformed/truncated
//! streams) bails with `Err`; the caller falls back to the
//! `jpeg-decoder` scaled path.

use super::scale::Bitmap;
use anyhow::{Result, bail};

/// Matches the engine-wide decompression-bomb guard.
const MAX_SOURCE_PIXELS: u64 = 300_000_000;

/// (dc bytes decoded, AC bytes skipped) of the last decode (test hook).
#[cfg(test)]
static LAST_SKIP: std::sync::Mutex<Option<(usize, usize)>> = std::sync::Mutex::new(None);

pub fn decode_dc_only(path: &std::path::Path) -> Result<Bitmap> {
    let data = std::fs::read(path)?;
    decode_dc_only_bytes(&data)
}

/// Decode a progressive JPEG to its 1/8-scale RGB bitmap from DC scans only.
pub fn decode_dc_only_bytes(data: &[u8]) -> Result<Bitmap> {
    Decoder::new(data)?.decode()
}

// ---------------------------------------------------------------------------
// Marker parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct ComponentSpec {
    id: u8,
    sx: u8,
    sy: u8,
    quant_id: u8,
}

#[derive(Clone)]
struct Frame {
    width: u32,
    height: u32,
    comps: Vec<ComponentSpec>,
    hmax: u8,
    vmax: u8,
}

impl Frame {
    fn block_dims(&self, c: usize) -> (usize, usize) {
        let comp = &self.comps[c];
        let bw = (u64::from(self.width) * u64::from(comp.sx)).div_ceil(u64::from(self.hmax) * 8)
            as usize;
        let bh = (u64::from(self.height) * u64::from(comp.sy)).div_ceil(u64::from(self.vmax) * 8)
            as usize;
        (bw.max(1), bh.max(1))
    }
}

/// A DC scan's decode context, captured at SOS time.
struct ScanCtx {
    /// Component indices participating, in scan order.
    comps: Vec<usize>,
    /// DC huffman table id per participating component (parallel to comps).
    tables: Vec<usize>,
    ah: u8,
    al: u8,
    interleaved: bool,
}

struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
    frame: Option<Frame>,
    coefs: Vec<Vec<i16>>,
    quant: [Vec<u16>; 4],
    dc_tables: [Option<HuffTable>; 4],
    restart_interval: usize,
    /// APP14 Adobe transform flag: 0 = RGB, 1/absent = YCbCr.
    rgb_transform: bool,
    #[cfg(test)]
    dc_bytes: usize,
    #[cfg(test)]
    skipped_bytes: usize,
}

impl<'a> Decoder<'a> {
    fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
            bail!("not a JPEG stream");
        }
        Ok(Decoder {
            data,
            pos: 2,
            frame: None,
            coefs: Vec::new(),
            quant: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            dc_tables: [None, None, None, None],
            restart_interval: 0,
            rgb_transform: false,
            #[cfg(test)]
            dc_bytes: 0,
            #[cfg(test)]
            skipped_bytes: 0,
        })
    }

    fn decode(mut self) -> Result<Bitmap> {
        let mut saw_dc_scan = false;
        #[allow(clippy::while_let_loop)]
        loop {
            let Some((m, seg_start, seg_len)) = self.next_marker() else {
                break; // out of data: reconstruct from what we have
            };
            self.pos = seg_start + seg_len;
            match m {
                0xD9 => break, // EOI
                0xC2 => self.parse_sof(seg_start, seg_len)?,
                0xC4 => self.parse_dht(seg_start, seg_len)?,
                0xDB => self.parse_dqt(seg_start, seg_len)?,
                0xDD => {
                    let s = self.seg(seg_start, seg_len)?;
                    if s.len() < 2 {
                        bail!("short DRI");
                    }
                    self.restart_interval = usize::from(u16::from_be_bytes([s[0], s[1]]));
                }
                0xEE => {
                    // APP14 Adobe: transform flag at payload offset 11.
                    let s = self.seg(seg_start, seg_len)?;
                    if s.len() >= 12 && &s[..5] == b"Adobe" && s[11] == 0 {
                        self.rgb_transform = true;
                    }
                }
                0xDA => match self.parse_sos(seg_start, seg_len)? {
                    Some(ctx) => {
                        self.decode_dc_scan(&ctx)?;
                        saw_dc_scan = true;
                    }
                    None => {
                        // AC scan: skip its entropy payload byte-wise.
                        #[allow(unused_variables)]
                        let before = self.pos;
                        self.skip_scan_payload();
                        #[cfg(test)]
                        {
                            self.skipped_bytes += self.pos - before;
                        }
                    }
                },
                0xC0 | 0xC1 | 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                    bail!("unsupported SOF marker 0x{m:02X} (only progressive SOF2)")
                }
                0xCC => bail!("arithmetic coding (DAC) not supported"),
                _ => {} // APPn/COM/DNL/standalone: skipped by length
            }
        }
        let frame = self
            .frame
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no SOF2 frame found"))?;
        if !saw_dc_scan {
            bail!("no DC scan decoded");
        }
        let bitmap = self.reconstruct(&frame)?;
        #[cfg(test)]
        {
            *LAST_SKIP.lock().unwrap() = Some((self.dc_bytes, self.skipped_bytes));
        }
        Ok(bitmap)
    }

    fn seg(&self, start: usize, len: usize) -> Result<&'a [u8]> {
        self.data
            .get(start..start + len)
            .ok_or_else(|| anyhow::anyhow!("segment exceeds buffer"))
    }

    /// Advance to the next marker. Returns `(marker, seg_start, seg_len)`
    /// where seg_start/seg_len cover the parameter bytes (after the 2-byte
    /// length for length-prefixed markers; 0 for standalone markers).
    fn next_marker(&mut self) -> Option<(u8, usize, usize)> {
        let d = self.data;
        let mut i = self.pos;
        while i + 1 < d.len() {
            if d[i] != 0xFF {
                i += 1;
                continue;
            }
            // Fill bytes: repeated 0xFF padding before the marker code.
            let mut j = i + 1;
            while j < d.len() && d[j] == 0xFF {
                j += 1;
            }
            let m = *d.get(j)?;
            i = j - 1;
            if m == 0x00 {
                i += 2; // stuffed byte inside entropy data
                continue;
            }
            if m == 0x01 || (0xD0..=0xD7).contains(&m) || m == 0xD8 {
                return Some((m, j + 1, 0)); // standalone
            }
            let len = usize::from(u16::from_be_bytes([*d.get(j + 1)?, *d.get(j + 2)?]));
            if len < 2 {
                return Some((m, j + 1, 0)); // malformed; treat as standalone
            }
            return Some((m, j + 3, len - 2));
        }
        None
    }

    fn parse_sof(&mut self, start: usize, len: usize) -> Result<()> {
        if self.frame.is_some() {
            bail!("multiple SOF frames");
        }
        let s = self.seg(start, len)?;
        if s.len() < 6 {
            bail!("short SOF2");
        }
        if s[0] != 8 {
            bail!("only 8-bit precision supported");
        }
        let height = u16::from_be_bytes([s[1], s[2]]);
        let width = u16::from_be_bytes([s[3], s[4]]);
        let ncomp = s[5];
        if width == 0 || height == 0 {
            bail!("DNL-height streams not supported");
        }
        if ncomp != 1 && ncomp != 3 {
            bail!("{ncomp}-component JPEG not supported (1 or 3 only)");
        }
        if s.len() < 6 + 3 * ncomp as usize {
            bail!("short SOF2 component list");
        }
        let mut comps = Vec::with_capacity(ncomp as usize);
        for c in 0..ncomp as usize {
            let b = &s[6 + c * 3..];
            let (sx, sy) = (b[1] >> 4, b[1] & 0x0F);
            if sx == 0 || sy == 0 || sx > 4 || sy > 4 {
                bail!("bad sampling factors {sx}x{sy}");
            }
            comps.push(ComponentSpec {
                id: b[0],
                sx,
                sy,
                quant_id: b[2],
            });
        }
        let hmax = comps.iter().map(|c| c.sx).max().unwrap();
        let vmax = comps.iter().map(|c| c.sy).max().unwrap();
        if u64::from(width) * u64::from(height) > MAX_SOURCE_PIXELS {
            bail!("source {width}x{height} exceeds pixel guard");
        }
        let frame = Frame {
            width: u32::from(width),
            height: u32::from(height),
            comps,
            hmax,
            vmax,
        };
        self.coefs = (0..frame.comps.len())
            .map(|c| {
                let (bw, bh) = frame.block_dims(c);
                vec![0i16; bw * bh]
            })
            .collect();
        self.frame = Some(frame);
        Ok(())
    }

    fn parse_dqt(&mut self, start: usize, len: usize) -> Result<()> {
        let s = self.seg(start, len)?;
        let mut i = 0;
        while i < s.len() {
            let (pq, tq) = (s[i] >> 4, s[i] & 0x0F);
            if pq != 0 {
                bail!("16-bit quantization tables not supported");
            }
            let Some(t) = s.get(i + 1..i + 65) else {
                bail!("short DQT");
            };
            self.quant[tq as usize] = t.iter().map(|&v| u16::from(v)).collect();
            i += 65;
        }
        Ok(())
    }

    fn parse_dht(&mut self, start: usize, len: usize) -> Result<()> {
        let s = self.seg(start, len)?;
        let mut i = 0;
        while i < s.len() {
            let (class, id) = (s[i] >> 4, s[i] & 0x0F);
            let Some(hdr) = s.get(i + 1..i + 17) else {
                bail!("short DHT");
            };
            let counts: [u8; 16] = hdr.try_into().unwrap();
            let total: usize = counts.iter().map(|&c| c as usize).sum();
            if total == 0 || total > 256 || i + 17 + total > s.len() {
                bail!("bad DHT code count");
            }
            if class == 0 {
                // Only DC tables are needed; AC tables parse+skip.
                let vals = s[i + 17..i + 17 + total].to_vec();
                self.dc_tables[id as usize] = Some(HuffTable::build(&counts, &vals)?);
            }
            i += 17 + total;
        }
        Ok(())
    }

    /// Parse an SOS header. Returns `Some(ctx)` for a DC scan (decode it),
    /// `None` for an AC scan (skip its payload). Positions `self.pos` at
    /// the first entropy byte.
    fn parse_sos(&mut self, start: usize, len: usize) -> Result<Option<ScanCtx>> {
        let Some(frame) = &self.frame else {
            bail!("SOS before SOF2");
        };
        let s = self.seg(start, len)?;
        if s.is_empty() {
            bail!("short SOS");
        }
        let ns = s[0] as usize;
        if ns == 0 || ns > frame.comps.len() || s.len() < 1 + ns * 2 + 3 {
            bail!("bad SOS component count");
        }
        let mut comps = Vec::with_capacity(ns);
        let mut tables = Vec::with_capacity(ns);
        for c in 0..ns {
            let cid = s[1 + c * 2];
            let idx = frame
                .comps
                .iter()
                .position(|fc| fc.id == cid)
                .ok_or_else(|| anyhow::anyhow!("SOS references unknown component {cid}"))?;
            if comps.contains(&idx) {
                bail!("duplicate component in scan");
            }
            comps.push(idx);
            tables.push(usize::from(s[2 + c * 2] >> 4)); // DC table selector
        }
        let tail = &s[1 + ns * 2..];
        let (ss, se, ah, al) = (tail[0], tail[1], tail[2] >> 4, tail[2] & 0x0F);
        if ss == 0 && se == 0 {
            if ah > 1 || al > 15 {
                bail!("bad DC scan Ah/Al {ah}/{al}");
            }
            Ok(Some(ScanCtx {
                comps,
                tables,
                ah,
                al,
                interleaved: ns > 1,
            }))
        } else if ss >= 1 {
            // The 1/8 output is independent of every AC coefficient.
            Ok(None)
        } else {
            bail!("invalid spectral band {ss}..{se}")
        }
    }

    /// Skip entropy bytes until the next marker that is not a stuffed byte
    /// or a restart marker.
    fn skip_scan_payload(&mut self) {
        let d = self.data;
        let mut i = self.pos;
        while i + 1 < d.len() {
            if d[i] == 0xFF {
                let m = d[i + 1];
                if m != 0x00 && !(0xD0..=0xD7).contains(&m) {
                    break;
                }
                i += 2; // 0xFF00 stuffing or an in-scan restart marker
                continue;
            }
            i += 1;
        }
        self.pos = i.min(d.len().saturating_sub(1));
    }

    // -----------------------------------------------------------------------
    // DC scan decoding
    // -----------------------------------------------------------------------

    fn decode_dc_scan(&mut self, ctx: &ScanCtx) -> Result<()> {
        let frame = self
            .frame
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no frame"))?;
        let mut reader = Bits::new(self.data, self.pos);
        // Snapshot the huffman tables this scan needs (avoids &mut self
        // inside the block loop).
        let tables: Vec<Option<HuffTable>> = ctx
            .tables
            .iter()
            .map(|&t| self.dc_tables[t].clone())
            .collect();
        let mut pred = vec![0i32; frame.comps.len()];

        if ctx.interleaved {
            let mcus_x = (u64::from(frame.width).div_ceil(u64::from(frame.hmax) * 8)) as usize;
            let mcus_y = (u64::from(frame.height).div_ceil(u64::from(frame.vmax) * 8)) as usize;
            'mcu: for my in 0..mcus_y {
                for mx in 0..mcus_x {
                    let mcu = my * mcus_x + mx;
                    if self.restart_interval > 0
                        && mcu > 0
                        && mcu.is_multiple_of(self.restart_interval)
                    {
                        reader.align_to_byte();
                        if !reader.consume_restart() {
                            break 'mcu; // truncated: keep what we have
                        }
                        pred.iter_mut().for_each(|p| *p = 0);
                    }
                    for (si, &c) in ctx.comps.iter().enumerate() {
                        let comp = &frame.comps[c];
                        let (bw, bh) = frame.block_dims(c);
                        for by in 0..comp.sy as usize {
                            for bx in 0..comp.sx as usize {
                                let (gx, gy) =
                                    (mx * comp.sx as usize + bx, my * comp.sy as usize + by);
                                if gx >= bw || gy >= bh {
                                    // Dummy block past the image edge: entropy
                                    // coded (must consume) but discarded.
                                    read_dc_block(&mut reader, &tables[si], ctx, &mut pred[c])?;
                                    continue;
                                }
                                let v = read_dc_block(&mut reader, &tables[si], ctx, &mut pred[c])?;
                                let idx = gy * bw + gx;
                                if ctx.ah > 0 {
                                    self.coefs[c][idx] |= v;
                                } else {
                                    self.coefs[c][idx] = v;
                                }
                            }
                        }
                    }
                }
            }
        } else {
            for (si, &c) in ctx.comps.iter().enumerate() {
                let (bw, bh) = frame.block_dims(c);
                let n = bw * bh;
                for i in 0..n {
                    if self.restart_interval > 0 && i > 0 && i.is_multiple_of(self.restart_interval)
                    {
                        reader.align_to_byte();
                        if !reader.consume_restart() {
                            break;
                        }
                        pred[c] = 0;
                    }
                    let v = read_dc_block(&mut reader, &tables[si], ctx, &mut pred[c])?;
                    if ctx.ah > 0 {
                        self.coefs[c][i] |= v;
                    } else {
                        self.coefs[c][i] = v;
                    }
                }
            }
        }
        let end = reader.pos_after();
        #[cfg(test)]
        {
            self.dc_bytes += end.saturating_sub(self.pos);
        }
        self.pos = end;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Reconstruction: DC grids -> samples -> upsample -> color -> Bitmap
    // -----------------------------------------------------------------------

    fn reconstruct(&self, frame: &Frame) -> Result<Bitmap> {
        let n = frame.comps.len();
        let mut planes: Vec<Vec<u8>> = Vec::with_capacity(n);
        let mut dims: Vec<(usize, usize)> = Vec::with_capacity(n);
        for (c, comp) in frame.comps.iter().enumerate() {
            let (bw, bh) = frame.block_dims(c);
            let q = &self.quant[comp.quant_id as usize];
            if q.is_empty() {
                bail!("missing quantization table");
            }
            // (dc * q + 1024) / 8, clamped — jpeg-decoder's 1x1 IDCT.
            let plane: Vec<u8> = self.coefs[c]
                .iter()
                .map(|&dc| (i32::from(dc) * i32::from(q[0]) + 1024) / 8)
                .map(|v| v.clamp(0, 255) as u8)
                .collect();
            debug_assert_eq!(plane.len(), bw * bh);
            planes.push(plane);
            dims.push((bw, bh));
        }

        let (ow, oh) = dims[0];
        let mut out = vec![0u8; ow * oh * 3];
        let mut yrow = vec![0u8; ow];
        let mut cbrow = vec![0u8; ow];
        let mut crrow = vec![0u8; ow];

        for y in 0..oh {
            upsample_row(
                &planes[0],
                dims[0].0,
                dims[0].1,
                y,
                &frame.comps[0],
                frame,
                &mut yrow,
            );
            let row = &mut out[y * ow * 3..(y + 1) * ow * 3];
            if n == 3 {
                upsample_row(
                    &planes[1],
                    dims[1].0,
                    dims[1].1,
                    y,
                    &frame.comps[1],
                    frame,
                    &mut cbrow,
                );
                upsample_row(
                    &planes[2],
                    dims[2].0,
                    dims[2].1,
                    y,
                    &frame.comps[2],
                    frame,
                    &mut crrow,
                );
                if self.rgb_transform {
                    for x in 0..ow {
                        row[x * 3] = yrow[x];
                        row[x * 3 + 1] = cbrow[x];
                        row[x * 3 + 2] = crrow[x];
                    }
                } else {
                    for x in 0..ow {
                        let (r, g, b) = ycbcr_to_rgb(yrow[x], cbrow[x], crrow[x]);
                        row[x * 3] = r;
                        row[x * 3 + 1] = g;
                        row[x * 3 + 2] = b;
                    }
                }
            } else {
                for x in 0..ow {
                    row[x * 3] = yrow[x];
                    row[x * 3 + 1] = yrow[x];
                    row[x * 3 + 2] = yrow[x];
                }
            }
        }
        Ok(Bitmap::new_rgb(out, ow as u32, oh as u32))
    }
}

/// Decode one DC block value: huffman category + diff (first pass) or one
/// correction bit (refinement pass). Returns the coefficient value to store.
fn read_dc_block(
    reader: &mut Bits,
    table: &Option<HuffTable>,
    ctx: &ScanCtx,
    pred: &mut i32,
) -> Result<i16> {
    if ctx.ah == 0 {
        let t = huff_decode(reader, table)?;
        let diff = if t > 0 {
            let v = reader
                .receive(u32::from(t))
                .ok_or_else(|| anyhow::anyhow!("truncated DC diff"))? as i32;
            extend(v, u32::from(t))
        } else {
            0
        };
        *pred += diff;
        let v = *pred << ctx.al;
        Ok(v.clamp(-32768, 32767) as i16)
    } else {
        // DC refinement: one raw correction bit per block.
        let bit = reader
            .receive(1)
            .ok_or_else(|| anyhow::anyhow!("truncated DC refinement scan"))?
            as i16;
        Ok(bit << ctx.al)
    }
}

/// Produce one upsampled output row for a component, mirroring
/// jpeg-decoder's upsample_row math (copy at 1x, fir at 2x, nearest else).
fn upsample_row(
    plane: &[u8],
    bw: usize,
    bh: usize,
    y: usize,
    comp: &ComponentSpec,
    frame: &Frame,
    out: &mut [u8],
) {
    let ow = out.len();
    let hf = usize::from(frame.hmax / comp.sx.max(1));
    let vf = usize::from(frame.vmax / comp.sy.max(1));
    match (hf, vf) {
        (1, 1) => {
            let src = &plane[y.min(bh - 1) * bw..];
            let n = ow.min(bw);
            out[..n].copy_from_slice(&src[..n]);
        }
        (2, 1) => h2v1_row(plane, bw, bh, y, out),
        (2, 2) => h2v2_row(plane, bw, bh, y, out),
        _ => {
            // Generic (e.g. 4:1:1): nearest.
            let sy = (y * bh / ow.max(1)).min(bh - 1);
            let src = &plane[sy * bw..(sy + 1) * bw];
            for (x, o) in out.iter_mut().enumerate() {
                *o = src[(x * bw / ow.max(1)).min(bw - 1)];
            }
        }
    }
    // The fir kernels fill exactly 2*bw columns; the luma grid can be wider
    // (ceil rounding). Replicate the last sample into any tail columns.
    if ow > 2 * bw {
        let last = out[2 * bw - 1];
        for o in out.iter_mut().skip(2 * bw) {
            *o = last;
        }
    }
}

/// jpeg-decoder `UpsamplerH2V1`: 3/4 center + 1/4 neighbor.
fn h2v1_row(plane: &[u8], bw: usize, bh: usize, y: usize, out: &mut [u8]) {
    let inp = &plane[y.min(bh - 1) * bw..];
    let ow = out.len();
    if bw == 1 {
        out.fill(inp[0]);
        return;
    }
    let w = ow.min(bw * 2);
    out[0] = inp[0];
    if w > 1 {
        out[1] = ((3 * u32::from(inp[0]) + u32::from(inp[1]) + 2) >> 2) as u8;
    }
    for i in 1..bw - 1 {
        let s = 3 * u32::from(inp[i]) + 2;
        if i * 2 < w {
            out[i * 2] = ((s + u32::from(inp[i - 1])) >> 2) as u8;
        }
        if i * 2 + 1 < w {
            out[i * 2 + 1] = ((s + u32::from(inp[i + 1])) >> 2) as u8;
        }
    }
    if (bw - 1) * 2 < w {
        out[(bw - 1) * 2] = ((3 * u32::from(inp[bw - 1]) + u32::from(inp[bw - 2]) + 2) >> 2) as u8;
    }
    if (bw - 1) * 2 + 1 < w {
        out[(bw - 1) * 2 + 1] = inp[bw - 1];
    }
}

/// jpeg-decoder `UpsamplerH2V2`: even output rows blend the row above, odd
/// rows the row below, combined with a horizontal fir.
fn h2v2_row(plane: &[u8], bw: usize, bh: usize, y: usize, out: &mut [u8]) {
    let ow = out.len();
    let near = (y / 2).min(bh - 1);
    let far = if y.is_multiple_of(2) {
        near.saturating_sub(1)
    } else {
        (near + 1).min(bh - 1)
    };
    let rn = &plane[near * bw..];
    let rf = &plane[far * bw..];
    if bw == 1 {
        let v = ((3 * u32::from(rn[0]) + u32::from(rf[0]) + 2) >> 2) as u8;
        out.fill(v);
        return;
    }
    let w = ow.min(bw * 2);
    let mut t1 = 3 * u32::from(rn[0]) + u32::from(rf[0]);
    out[0] = ((t1 + 2) >> 2) as u8;
    for i in 1..bw {
        let t0 = t1;
        t1 = 3 * u32::from(rn[i]) + u32::from(rf[i]);
        if i * 2 - 1 < w {
            out[i * 2 - 1] = ((3 * t0 + t1 + 8) >> 4) as u8;
        }
        if i * 2 < w {
            out[i * 2] = ((3 * t1 + t0 + 8) >> 4) as u8;
        }
    }
    if bw * 2 - 1 < w {
        out[bw * 2 - 1] = ((t1 + 2) >> 2) as u8;
    }
}

// ---------------------------------------------------------------------------
// Huffman + bit reader
// ---------------------------------------------------------------------------

struct HuffTable {
    /// 8-bit fast lookup: `(len << 4) | min(category, 15)`; 0 = slow path.
    fast: [u8; 256],
    mincode: [i32; 17],
    maxcode: [i32; 17],
    valptr: [i32; 17],
    vals: Vec<u8>,
}

impl Clone for HuffTable {
    fn clone(&self) -> Self {
        HuffTable {
            fast: self.fast,
            mincode: self.mincode,
            maxcode: self.maxcode,
            valptr: self.valptr,
            vals: self.vals.clone(),
        }
    }
}

impl HuffTable {
    fn build(counts: &[u8; 16], vals: &[u8]) -> Result<Self> {
        let mut mincode = [0i32; 17];
        let mut maxcode = [-1i32; 17];
        let mut valptr = [0i32; 17];
        let mut fast = [0u8; 256];
        let mut code: i32 = 0;
        let mut k: i32 = 0;
        for len in 1..=16usize {
            valptr[len] = k;
            mincode[len] = code;
            let cnt = i32::from(counts[len - 1]);
            if len <= 8 {
                for i in 0..cnt {
                    let v = vals[(k + i) as usize];
                    let start = ((code + i) as u32) << (8 - len);
                    let span = 1u32 << (8 - len);
                    for e in fast
                        .iter_mut()
                        .take((start + span) as usize)
                        .skip(start as usize)
                    {
                        *e = ((len as u8) << 4) | v.min(15);
                    }
                }
            }
            code += cnt;
            k += cnt;
            maxcode[len] = code - 1;
            code <<= 1;
        }
        Ok(HuffTable {
            fast,
            mincode,
            maxcode,
            valptr,
            vals: vals.to_vec(),
        })
    }
}

fn huff_decode(bits: &mut Bits, table: &Option<HuffTable>) -> Result<u8> {
    let Some(t) = table else {
        bail!("missing DC huffman table");
    };
    // Fast path: peek 8 bits, commit only the code length.
    let peek = bits
        .peek8()
        .ok_or_else(|| anyhow::anyhow!("truncated huffman stream"))?;
    let entry = t.fast[peek as usize];
    if entry != 0 {
        bits.skip(u32::from(entry >> 4))?;
        return Ok(entry & 0x0F);
    }
    // Slow path: canonical search for codes longer than 8 bits.
    let mut code = 0i32;
    for len in 1..=16usize {
        code = (code << 1)
            | bits
                .bit()
                .ok_or_else(|| anyhow::anyhow!("truncated huffman"))? as i32;
        if t.maxcode[len] >= code && code >= t.mincode[len] {
            let idx = (t.valptr[len] + code - t.mincode[len]) as usize;
            return Ok(*t
                .vals
                .get(idx)
                .ok_or_else(|| anyhow::anyhow!("huffman index out of range"))?);
        }
    }
    bail!("invalid huffman code")
}

/// JPEG entropy bit reader with 0xFF00 unstuffing and marker detection.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    /// Bits left-aligned at the top of `cur`.
    cur: u32,
    cnt: i32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Bits {
            data,
            pos,
            cur: 0,
            cnt: 0,
        }
    }

    fn fetch_byte(&mut self) -> Option<u8> {
        let &b = self.data.get(self.pos)?;
        if b == 0xFF {
            match self.data.get(self.pos + 1) {
                Some(&0x00) => {
                    self.pos += 2;
                    Some(0xFF)
                }
                _ => None, // marker: stop without consuming
            }
        } else {
            self.pos += 1;
            Some(b)
        }
    }

    fn bit(&mut self) -> Option<u32> {
        if self.cnt == 0 {
            self.cur = u32::from(self.fetch_byte()?) << 24;
            self.cnt = 8;
        }
        let b = (self.cur >> 31) & 1;
        self.cur <<= 1;
        self.cnt -= 1;
        Some(b)
    }

    fn receive(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Some(v)
    }

    fn peek8(&mut self) -> Option<u8> {
        loop {
            if self.cnt >= 8 {
                return Some((self.cur >> 24) as u8);
            }
            match self.fetch_byte() {
                Some(b) => {
                    self.cur |= u32::from(b) << (24 - self.cnt);
                    self.cnt += 8;
                }
                None => {
                    // Stream/marker reached mid-peek: pad the missing low
                    // bits with 1s (JPEG end-of-stream convention). A code
                    // that actually needs those bits will fail in `skip`.
                    let pad = 8 - self.cnt;
                    let v = ((self.cur >> 24) as u8) | (((1u16 << pad) - 1) as u8);
                    return Some(v);
                }
            }
        }
    }

    fn skip(&mut self, n: u32) -> Result<()> {
        for _ in 0..n {
            self.bit()
                .ok_or_else(|| anyhow::anyhow!("truncated bits"))?;
        }
        Ok(())
    }

    /// Discard bits to the byte boundary and rewind whole unconsumed bytes
    /// (used at restart markers).
    fn align_to_byte(&mut self) {
        self.pos -= (self.cnt / 8) as usize;
        self.cur = 0;
        self.cnt = 0;
    }

    /// After the reader stopped at a marker at a restart boundary: consume
    /// the RSTn marker and resume. Returns false when none follows.
    fn consume_restart(&mut self) -> bool {
        if self.data.get(self.pos) != Some(&0xFF) {
            return false;
        }
        let m = self.data.get(self.pos + 1).copied().unwrap_or(0);
        if (0xD0..=0xD7).contains(&m) {
            self.pos += 2;
            self.cur = 0;
            self.cnt = 0;
            true
        } else {
            false
        }
    }

    /// Stream position after the last consumed bit (whole unconsumed bytes
    /// held in `cur` are rewound).
    fn pos_after(&self) -> usize {
        self.pos.saturating_sub((self.cnt / 8) as usize)
    }
}

/// JPEG `Extend`: map an n-bit magnitude to its signed value.
fn extend(v: i32, n: u32) -> i32 {
    if v < (1 << (n - 1)) {
        v - (1 << n) + 1
    } else {
        v
    }
}

/// jpeg-decoder's fixed-point YCbCr→RGB (offset 2^20, truncating shift).
fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
    const FP: i32 = 20;
    const HALF: i32 = 1 << 19;
    let f2f = |x: f32| -> i32 { (x * (1u32 << FP) as f32 + 0.5) as i32 };
    let clamp = |v: i32| -> u8 { (v >> FP).clamp(0, 255) as u8 };
    let (cb, cr) = (i32::from(cb) - 128, i32::from(cr) - 128);
    let yf = i32::from(y) * (1 << FP) + HALF;
    (
        clamp(yf + f2f(1.40200) * cr),
        clamp(yf - f2f(0.34414) * cb - f2f(0.71414) * cr),
        clamp(yf + f2f(1.77200) * cb),
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/media/thumb/pjpeg.rs"]
mod tests;
