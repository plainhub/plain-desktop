import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const { native, invoke, remote, save } = vi.hoisted(() => ({ native: vi.fn(), invoke: vi.fn(), remote: vi.fn(), save: vi.fn() }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: native }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: remote }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ save }))

import { revealLocalFile, saveLocalFileAs, saveTextFileAs, copyLocalFile } from '../../src/lib/native-file-actions'

beforeEach(() => vi.resetAllMocks())
afterEach(() => vi.restoreAllMocks())

it('resolves references through native Rust before invoking OS file actions', async () => {
  invoke.mockImplementation(async (command) => command === 'native_file_paths' ? ['/data/files/resolved.txt'] : undefined)
  await revealLocalFile('fid:attachment.txt')
  await saveLocalFileAs('app://notes/report.txt', 'Report.txt')
  await copyLocalFile('file:///data/document%20name.txt')
  expect(invoke.mock.calls).toEqual([
    ['native_file_paths', { paths: ['fid:attachment.txt'] }],
    ['reveal_file', { path: '/data/files/resolved.txt' }],
    ['native_file_paths', { paths: ['app://notes/report.txt'] }],
    ['save_file_as', { path: '/data/files/resolved.txt', name: 'Report.txt' }],
    ['native_file_paths', { paths: ['file:///data/document%20name.txt'] }],
    ['copy_file_to_clipboard', { path: '/data/files/resolved.txt' }],
  ])
  expect(native).not.toHaveBeenCalled()
  expect(remote).not.toHaveBeenCalled()
})

it('never invokes an OS file action after an unresolved or refused reference', async () => {
  for (const response of [undefined, null, [null], [], '/invalid-shape', [42]]) {
    invoke.mockResolvedValueOnce(response)
    await expect(saveLocalFileAs('fid:missing', 'Missing.txt')).rejects.toThrow()
  }
  invoke.mockRejectedValueOnce(new Error('Read denied'))
  await expect(saveLocalFileAs('fid:missing', 'Missing.txt')).rejects.toThrow('Read denied')
  expect(invoke.mock.calls.every(([command]) => command === 'native_file_paths')).toBe(true)
})

it('propagates OS action failures to the calling UI', async () => {
  invoke.mockResolvedValueOnce(['/data/document.txt']).mockRejectedValueOnce(new Error('File no longer exists'))
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
