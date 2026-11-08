pub(super) fn dib_v5(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    const HEADER_SIZE: usize = 124;
    let invalid = || "Clipboard image dimensions are invalid".to_string();
    let (width, height) = (
        i32::try_from(width).map_err(|_| invalid())?,
        i32::try_from(height).map_err(|_| invalid())?,
    );
    let pixels = width as u64 * height as u64;
    let size = u32::try_from(pixels * 4).map_err(|_| invalid())?;
    if size == 0 || rgba.len() != size as usize {
        return Err(invalid());
    }

    let mut dib = Vec::with_capacity(HEADER_SIZE.checked_add(rgba.len()).ok_or_else(invalid)?);
    dib.resize(HEADER_SIZE, 0);
    dib[0..4].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
    dib[4..8].copy_from_slice(&width.to_le_bytes());
    dib[8..12].copy_from_slice(&(-height).to_le_bytes());
    dib[12..14].copy_from_slice(&1u16.to_le_bytes());
    dib[14..16].copy_from_slice(&32u16.to_le_bytes());
    dib[16..20].copy_from_slice(&3u32.to_le_bytes()); // BI_BITFIELDS
    dib[20..24].copy_from_slice(&size.to_le_bytes());
    for (index, mask) in [0x00ff0000u32, 0x0000ff00, 0x000000ff, 0xff000000]
        .into_iter()
        .enumerate()
    {
        let offset = 40 + 4 * index;
        dib[offset..offset + 4].copy_from_slice(&mask.to_le_bytes());
    }
    dib[56..60].copy_from_slice(&0x73524742u32.to_le_bytes()); // LCS_sRGB
    dib[108..112].copy_from_slice(&4u32.to_le_bytes()); // LCS_GM_IMAGES
    for pixel in rgba.chunks_exact(4) {
        dib.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    Ok(dib)
}

#[cfg(test)]
#[path = "../../../tests/unit/host/clipboard_dib.rs"]
mod tests;
