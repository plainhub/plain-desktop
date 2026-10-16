import { afterEach, beforeEach, expect, it, vi } from 'vitest'

const { native, invoke, remote } = vi.hoisted(() => ({ native: vi.fn(), invoke: vi.fn(), remote: vi.fn() }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: native }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: remote }))

import { revealLocalFile, saveLocalFileAs, copyLocalFile } from '../../src/lib/native-file-actions'

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
