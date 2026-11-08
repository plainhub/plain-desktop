import { deflateSync, inflateRawSync } from 'node:zlib'
import { afterEach, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { ZipWriter } from '@/lib/zip-writer'

const fixture = vi.hoisted(() => ({
  download: vi.fn(),
  gqlFetch: vi.fn(),
  toast: vi.fn(),
  popModal: vi.fn(),
  unmount: vi.fn<() => void>(),
}))

vi.mock('vue', async (original) => ({
  ...await original<typeof import('vue')>(),
  onUnmounted: (callback: () => void) => { fixture.unmount.mockImplementation(callback) },
}))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/modal', () => ({ popModal: fixture.popModal }))
vi.mock('@/components/toaster', () => ({ default: fixture.toast }))
vi.mock('@/lib/api/file', () => ({
  download: fixture.download,
  getFileUrlByPath: (_key: unknown, path: string) => `https://device.test${path}`,
}))
vi.mock('@/lib/api/query', () => ({ smsGQL: 'legacy-sms-query' }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: fixture.gqlFetch }))
vi.mock('@/lib/prefs/appearance', () => ({ get: () => 'en-US' }))
vi.mock('@/hooks/list', () => ({ useDelete: () => ({}) }))
vi.mock('@/hooks/notes', () => ({ useNotesRestore: () => ({}), useNotesTrash: () => ({}) }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/plugins/router', () => ({ default: {}, replacePath: vi.fn() }))

import { useExportSms } from '@/hooks/export-sms'
import { exportNotesGQL } from '@/lib/api/mutation'
import { useNotesActions } from '@/views/notes/hooks/useNotesActions'

async function readZip(blob: Blob) {
  const bytes = Buffer.from(await blob.arrayBuffer())
  const end = bytes.length - 22
  expect(bytes.readUInt32LE(end)).toBe(0x06054b50)
  const entries = new Map<string, { contents: Buffer; method: number; crc: number }>()
  let directory = bytes.readUInt32LE(end + 16)
  for (let index = 0; index < bytes.readUInt16LE(end + 10); index++) {
    expect(bytes.readUInt32LE(directory)).toBe(0x02014b50)
    const size = bytes.readUInt32LE(directory + 20)
    const method = bytes.readUInt16LE(directory + 10)
    const nameLength = bytes.readUInt16LE(directory + 28)
    const name = bytes.toString('utf8', directory + 46, directory + 46 + nameLength)
    const local = bytes.readUInt32LE(directory + 42)
    expect(bytes.readUInt32LE(local)).toBe(0x04034b50)
    expect(bytes.readUInt16LE(local + 6) & 0x0800).toBe(0x0800)
    expect(bytes.toString('utf8', local + 30, local + 30 + bytes.readUInt16LE(local + 26))).toBe(name)
    const dataOffset = local + 30 + bytes.readUInt16LE(local + 26) + bytes.readUInt16LE(local + 28)
    const data = bytes.subarray(dataOffset, dataOffset + size)
    const contents = method === 8 ? inflateRawSync(data) : data
    expect(contents.length).toBe(bytes.readUInt32LE(directory + 24))
    entries.set(name, { contents, method, crc: bytes.readUInt32LE(directory + 16) })
    directory += 46 + nameLength + bytes.readUInt16LE(directory + 30) + bytes.readUInt16LE(directory + 32)
  }
  expect(directory).toBe(end)
  return entries
}

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  vi.clearAllMocks()
})

it('compresses files with UTF-8 names, CRC32 and case-insensitive unique names', async () => {
  const zip = new ZipWriter()
  zip.addFile(['中文.md'], '123456789')
  zip.addFile(['hello.md'], 'first')
  zip.addFile(['HELLO.md'], 'second')
  zip.addFile(['empty.md'], '')
  zip.addDirectory(['attachments'])
  const entries = await readZip(await zip.generate())
  expect(entries.get('中文.md')).toEqual({ method: 8, contents: Buffer.from('123456789'), crc: 0xcbf43926 })
  expect(entries.get('hello.md')?.contents.toString()).toBe('first')
  expect(entries.get('HELLO (2).md')?.contents.toString()).toBe('second')
  expect(entries.get('empty.md')).toEqual({ method: 8, contents: Buffer.alloc(0), crc: 0 })
  expect(entries.get('attachments/')).toEqual({ method: 0, contents: Buffer.alloc(0), crc: 0 })
})

it('produces raw DEFLATE from a runtime that only provides zlib framing', async () => {
  const NativeCompressionStream = CompressionStream
  vi.stubGlobal('CompressionStream', class extends NativeCompressionStream {
    constructor(format: CompressionFormat) {
      if (format === 'deflate-raw') throw new TypeError('Unsupported format')
      super(format)
    }
  })
  const zip = new ZipWriter()
  const content = 'compressible text '.repeat(256)
  zip.addFile(['messages.txt'], content)
  const blob = await zip.generate()
  expect(blob.size).toBeLessThan(content.length)
  expect((await readZip(blob)).get('messages.txt')).toMatchObject({ method: 8, contents: Buffer.from(content) })
})

it('refuses a truncated native compression stream before generating a download', async () => {
  const content = 'synthetic text'.repeat(2048)
  const compressed = deflateSync(content)
  vi.stubGlobal('CompressionStream', class extends TransformStream {
    constructor() {
      super({ flush(controller) { controller.enqueue(compressed.subarray(0, -4)) } })
    }
  })
  const zip = new ZipWriter()
  zip.addFile(['messages.txt'], content)
  await expect(zip.generate()).rejects.toThrow('ZIP compression returned incomplete data')
})

it('rejects names and file sizes that cannot fit the ZIP32 fields', async () => {
  const zip = new ZipWriter()
  zip.addFile(['界'.repeat(21846)], 'content')
  await expect(zip.generate()).rejects.toThrow('ZIP archive is too large')

  const large = new ZipWriter()
  const file = new Blob()
  Object.defineProperty(file, 'size', { value: 0xffff_ffff })
  large.addFile(['large.dat'], file)
  await expect(large.generate()).rejects.toThrow('ZIP archive is too large')
})

it('rejects an entry count reserved for ZIP64', async () => {
  const zip = new ZipWriter()
  for (let i = 0; i < 0xffff; i++) zip.addFile([String(i)], '')
  await expect(zip.generate()).rejects.toThrow('Too many files')
})

it('exports old-device notes as compressed Markdown without overwriting duplicate titles', async () => {
  fixture.gqlFetch.mockResolvedValue({ data: { exportNotes: JSON.stringify([
    { title: '中文/标题', content: 'first' },
    { title: '中文/标题', content: 'second' },
  ]) } })
  const blobUrl = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:synthetic-notes')
  const actions = useNotesActions({
    items: ref([]), selectedIds: ref(['one', 'two']), realAllChecked: ref(false),
    q: ref(''), total: ref(2), clearSelection: vi.fn(), fetch: vi.fn(),
  })
  await actions.exportNotes2()

  expect(fixture.gqlFetch).toHaveBeenCalledWith(exportNotesGQL, { query: 'ids:one,two' }, { dedupe: false })
  expect(fixture.download).toHaveBeenCalledWith('blob:synthetic-notes', 'notes.zip')
  const entries = await readZip(blobUrl.mock.calls[0][0] as Blob)
  expect(entries.get('中文_标题.md')).toMatchObject({ method: 8, contents: Buffer.from('first') })
  expect(entries.get('中文_标题 (2).md')).toMatchObject({ method: 8, contents: Buffer.from('second') })
})

it.each(['json', 'text', 'csv'])('exports old-device SMS with compressed, safe attachment paths in %s format', async (format) => {
  const paths = ['/one', '/two', '/three', '/missing']
  const names = ['../照片.jpg', '..\\照片.jpg', 'CON.txt', '../absent.txt']
  fixture.gqlFetch.mockResolvedValue({ data: { sms: [{
    id: '../123', type: 'INBOX', sentAt: '2026-11-08T12:00:00Z', address: 'fixture', body: 'A test message',
    attachments: paths.map((path, index) => ({ path, name: names[index], contentType: 'application/octet-stream' })),
  }] } })
  const fetch = vi.fn((url: string) => Promise.resolve(url.endsWith('/missing') ? new Response(null, { status: 404 }) : new Response(url)))
  vi.stubGlobal('fetch', fetch)
  const blobUrl = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:synthetic-export')
  const exporter = useExportSms({ items: [], query: 'fixture-query', contactName: 'fixture', urlTokenKey: null })
  exporter.selectedFormat.value = format
  await exporter.doExport()

  expect(fixture.gqlFetch).toHaveBeenCalledWith('legacy-sms-query', { offset: 0, limit: 100000, query: 'fixture-query' })
  expect(fetch.mock.calls.map(([url]) => url)).toEqual(paths.map(path => `https://device.test${path}`))
  expect(fixture.toast).not.toHaveBeenCalled()
  expect(fixture.download).toHaveBeenCalledWith('blob:synthetic-export', 'fixture_sms.zip')
  const entries = await readZip(blobUrl.mock.calls[0][0] as Blob)
  const attachments = ['.._照片.jpg', '.._照片 (2).jpg', '_CON.txt'].map(name => `attachments/.._123/${name}`)
  expect([...entries.keys()].filter(name => !name.endsWith('/') && name.startsWith('attachments/'))).toEqual(attachments)
  expect(attachments.map(name => entries.get(name)?.contents.toString())).toEqual(paths.slice(0, 3).map(path => `https://device.test${path}`))
  const report = entries.get(`messages.${format === 'text' ? 'txt' : format}`)!
  expect(report.method).toBe(8)
  expect(entries.get(attachments[0])?.method).toBe(8)
  if (format === 'json') expect(JSON.parse(report.contents.toString())[0].attachmentFiles.slice(0, 3).map((file: { localPath: string }) => file.localPath)).toEqual(attachments)
  if (format === 'text') for (const path of attachments) expect(report.contents.toString()).toContain(`[attachment: ${path}]`)
  if (format === 'csv') expect(report.contents.toString()).toMatch(/^\uFEFF"date"/)
})

it('cancels an attachment fetch on dismissal without downloading an archive', async () => {
  fixture.gqlFetch.mockResolvedValue({ data: { sms: [{ id: '123', sentAt: '2026-11-08T12:00:00Z', attachments: [{ path: '/one', name: 'one' }] }] } })
  const fetch = vi.fn((_url: string, { signal }: { signal: AbortSignal }) => new Promise<Response>((_, reject) => {
    signal.addEventListener('abort', () => reject(signal.reason), { once: true })
  }))
  vi.stubGlobal('fetch', fetch)
  const exporter = useExportSms({ items: [], query: null, contactName: '', urlTokenKey: null })
  const pending = exporter.doExport()
  await vi.waitFor(() => expect(fetch).toHaveBeenCalledTimes(1))
  fixture.unmount()
  await pending
  expect(fixture.download).not.toHaveBeenCalled()
  expect(fixture.toast).not.toHaveBeenCalled()
  expect(exporter.exporting.value).toBe(false)
})
