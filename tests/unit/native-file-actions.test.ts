import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const { native, invoke, remote, save } = vi.hoisted(() => ({ native: vi.fn(), invoke: vi.fn(), remote: vi.fn(), save: vi.fn() }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: native }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: remote }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ save }))

import { revealLocalFile, saveLocalFileAs, saveTextFileAs, copyLocalFile } from '../../src/lib/native-file-actions'

beforeEach(() => vi.resetAllMocks())
afterEach(() => vi.restoreAllMocks())

it('resolves references in the local main schema before invoking OS file actions', async () => {
  native.mockResolvedValue({ data: { filePaths: ['/data/files/resolved.txt'] } })
  await revealLocalFile('fid:attachment.txt')
  await saveLocalFileAs('app://notes/report.txt', 'Report.txt')
  await copyLocalFile('file:///data/document%20name.txt')
  expect(native.mock.calls.map(([body]) => body.variables.paths)).toEqual([
    ['fid:attachment.txt'], ['app://notes/report.txt'], ['file:///data/document%20name.txt'],
  ])
  for (const [body] of native.mock.calls) expect(body.query).toContain('filePaths(paths: $paths)')
  expect(invoke.mock.calls).toEqual([
    ['reveal_file', { path: '/data/files/resolved.txt' }],
    ['save_file_as', { path: '/data/files/resolved.txt', name: 'Report.txt' }],
    ['copy_file_to_clipboard', { path: '/data/files/resolved.txt' }],
  ])
  expect(remote).not.toHaveBeenCalled()
})

it('never opens an OS dialog or clipboard action after an unresolved or refused reference', async () => {
  for (const response of [
    { errors: [{ message: 'no_permission' }] },
    {},
    { data: { filePaths: [null] } },
    { data: { filePaths: [] } },
    { data: { filePaths: '/invalid-shape' } },
    { data: { filePaths: [42] } },
  ]) {
    native.mockResolvedValueOnce(response)
    await expect(saveLocalFileAs('fid:missing', 'Missing.txt')).rejects.toThrow()
  }
  expect(invoke).not.toHaveBeenCalled()
})

it('propagates OS action failures to the calling UI', async () => {
  native.mockResolvedValue({ data: { filePaths: ['/data/document.txt'] } })
  invoke.mockRejectedValue(new Error('File no longer exists'))
  await expect(revealLocalFile('fid:document.txt')).rejects.toThrow('File no longer exists')
})

it('exports captured text with the selected path through the local main schema', async () => {
  save.mockResolvedValue('/exports/document.txt')
  native.mockResolvedValue({ data: { writeTextFile: { path: '/exports/document.txt' } } })
  const content = 'first\n\u6d4b\u8bd5\u0000last'
  await saveTextFileAs('  report.txt  ', content)
  expect(save).toHaveBeenCalledWith({ defaultPath: 'report.txt' })
  expect(native).toHaveBeenCalledTimes(1)
  expect(native.mock.calls[0][0].query).toContain('writeTextFile(path:$path,content:$content,overwrite:true)')
  expect(native.mock.calls[0][0].variables).toEqual({ path: '/exports/document.txt', content })
  expect(invoke).not.toHaveBeenCalled()
  expect(remote).not.toHaveBeenCalled()
})

it('does not write when the system save dialog is cancelled', async () => {
  save.mockResolvedValue(null)
  await saveTextFileAs('   ', 'unchanged')
  expect(save).toHaveBeenCalledWith({ defaultPath: 'file.txt' })
  expect(native).not.toHaveBeenCalled()
  expect(invoke).not.toHaveBeenCalled()
})

it('does not report a successful text export after a refused or missing result', async () => {
  save.mockResolvedValue('/exports/document.txt')
  for (const response of [{ errors: [{ message: 'Write denied' }] }, {}, { data: { writeTextFile: null } }]) {
    native.mockResolvedValueOnce(response)
    await expect(saveTextFileAs('document.txt', 'content')).rejects.toThrow()
  }
})
