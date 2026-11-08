type ZipContent = string | Blob

const maxOffset = 0xffff_ffff
const maxEntries = 0xfffe
const encoder = new TextEncoder()
const crcTable = Uint32Array.from({ length: 256 }, (_, byte) => {
  for (let bit = 0; bit < 8; bit++) byte = (byte >>> 1) ^ (byte & 1 ? 0xedb88320 : 0)
  return byte
})

function crc32(bytes: Uint8Array): number {
  let crc = 0xffff_ffff
  for (const byte of bytes) crc = (crc >>> 8) ^ crcTable[(crc ^ byte) & 0xff]
  return (crc ^ 0xffff_ffff) >>> 0
}

function safeSegment(name: string): string {
  const safe = Array.from(name.replace(/[\uD800-\uDFFF]/gu, '\uFFFD').normalize('NFC'), char =>
    char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127 || '/\\:*?"<>|'.includes(char) ? '_' : char,
  ).join('').replace(/[ .]+$/, '') || '_'
  return /^(?:con|prn|aux|nul|com[1-9¹²³]|lpt[1-9¹²³]) *(?:\.|$)/i.test(safe) ? `_${safe}` : safe
}

function adler32(bytes: Uint8Array): number {
  let a = 1
  let b = 0
  for (const byte of bytes) {
    a = (a + byte) % 65521
    b = (b + a) % 65521
  }
  return ((b << 16) | a) >>> 0
}

async function deflate(blob: Blob, checksum: number): Promise<Blob> {
  const wrapped = await new Response(blob.stream().pipeThrough(new CompressionStream('deflate'))).blob()
  // Check the zlib trailer before extracting raw DEFLATE because older WebKit can truncate the stream.
  if (wrapped.size < 8 || new DataView(await wrapped.slice(-4).arrayBuffer()).getUint32(0) !== checksum) {
    throw new Error('ZIP compression returned incomplete data')
  }
  return wrapped.slice(2, -4)
}

interface EntryHeader {
  name: Uint8Array
  method: number
  crc: number
  size: number
  compressedSize: number
  time: number
  date: number
  directory: boolean
}

function entryHeader(entry: EntryHeader, offset?: number): Uint8Array<ArrayBuffer> {
  const central = offset !== undefined
  const size = central ? 46 : 30
  const bytes = new Uint8Array(size + entry.name.length)
  const view = new DataView(bytes.buffer)
  const version = central ? 6 : 4
  view.setUint32(0, central ? 0x02014b50 : 0x04034b50, true)
  if (central) view.setUint16(4, 20, true)
  view.setUint16(version, 20, true)
  view.setUint16(version + 2, 0x0800, true)
  view.setUint16(version + 4, entry.method, true)
  view.setUint16(version + 6, entry.time, true)
  view.setUint16(version + 8, entry.date, true)
  view.setUint32(version + 10, entry.crc, true)
  view.setUint32(version + 14, entry.compressedSize, true)
  view.setUint32(version + 18, entry.size, true)
  view.setUint16(version + 22, entry.name.length, true)
  if (central) {
    view.setUint32(38, entry.directory ? 0x10 : 0, true)
    view.setUint32(42, offset, true)
  }
  bytes.set(entry.name, size)
  return bytes
}

export class ZipWriter {
  private entries = new Map<string, { path: string; content: ZipContent | null | undefined }>()
  private nextSuffix = new Map<string, number>()

  addDirectory(segments: readonly [string, ...string[]]): void {
    const path = `${segments.map(safeSegment).join('/')}/`
    const key = path.toLowerCase()
    if (!this.entries.has(key)) this.entries.set(key, { path, content: null })
  }

  addFile(segments: readonly [string, ...string[]], content?: ZipContent): string {
    const parts = segments.map(safeSegment)
    const name = parts.pop()!
    const prefix = parts.length ? `${parts.join('/')}/` : ''
    const original = prefix + name
    const key = original.toLowerCase()
    let path = original
    if (this.entries.has(key)) {
      const dot = name.lastIndexOf('.')
      const stem = dot > 0 ? name.slice(0, dot) : name
      const extension = dot > 0 ? name.slice(dot) : ''
      let suffix = this.nextSuffix.get(key) ?? 2
      do {
        path = `${prefix}${stem} (${suffix++})${extension}`
      } while (this.entries.has(path.toLowerCase()))
      this.nextSuffix.set(key, suffix)
    }
    this.entries.set(path.toLowerCase(), { path, content })
    return path
  }

  async generate(signal?: AbortSignal): Promise<Blob> {
    const entries = [...this.entries.values()].filter(entry => entry.content !== undefined)
    if (entries.length > maxEntries) throw new Error('Too many files for a ZIP archive')
    const now = new Date()
    const time = (now.getUTCHours() << 11) | (now.getUTCMinutes() << 5) | (now.getUTCSeconds() >> 1)
    const date = ((now.getUTCFullYear() - 1980) << 9) | ((now.getUTCMonth() + 1) << 5) | now.getUTCDate()
    const chunks: BlobPart[] = []
    const directory: Uint8Array<ArrayBuffer>[] = []
    let offset = 0
    let directorySize = 0

    for (const { path, content } of entries) {
      signal?.throwIfAborted()
      const name = encoder.encode(path)
      const file = content instanceof Blob ? content : new Blob(content == null ? [] : [content])
      if (name.length > 0xffff || file.size >= maxOffset) throw new Error('ZIP archive is too large')
      const bytes = new Uint8Array(await file.arrayBuffer())
      const crc = crc32(bytes)
      signal?.throwIfAborted()
      const isDirectory = content === null
      const data = isDirectory ? file : await deflate(file, adler32(bytes))
      const entry = { name, method: isDirectory ? 0 : 8, crc, time, date, size: file.size, compressedSize: data.size, directory: isDirectory }
      const local = entryHeader(entry)
      const central = entryHeader(entry, offset)
      offset += local.length + data.size
      directorySize += central.length
      if (offset + directorySize + 22 >= maxOffset) throw new Error('ZIP archive is too large')
      chunks.push(local, data)
      directory.push(central)
    }

    signal?.throwIfAborted()
    const end = new DataView(new ArrayBuffer(22))
    end.setUint32(0, 0x06054b50, true)
    end.setUint16(8, entries.length, true)
    end.setUint16(10, entries.length, true)
    end.setUint32(12, directorySize, true)
    end.setUint32(16, offset, true)
    return new Blob([...chunks, ...directory, end.buffer], { type: 'application/zip' })
  }
}
