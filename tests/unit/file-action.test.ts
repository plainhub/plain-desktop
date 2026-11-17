import { expect, it } from 'vitest'
import { FileAction, fileQuery, withFileAction } from '@/lib/api/file-action'

it('builds uppercase operation parameters and preserves preview options', () => {
  const query = new URLSearchParams(fileQuery(FileAction.Preview, '&size=SM&v=updated').slice(1))
  expect(query.get('action')).toBe('PREVIEW')
  expect(query.get('size')).toBe('SM')
  expect(query.get('v')).toBe('updated')
})

it.each([
  ['/fs', false],
  ['/proxy/c.self.ticket/fs', false],
  ['/proxy/peer/fs', true],
  ['/proxy/peer/fs', false],
])('downloads original bytes through %s and removes thumbnail sizing', (path, chat) => {
  const url = new URL(withFileAction(`${path}?id=encrypted%2Bvalue&action=PREVIEW&size=SM`, FileAction.Download, chat), 'http://localhost')
  expect(url.searchParams.get('id')).toBe('encrypted+value')
  expect(url.searchParams.getAll('action')).toEqual(['DOWNLOAD'])
  expect(url.searchParams.has('size')).toBe(false)
})

it('replaces the preview operation when probing and transcoding', () => {
  for (const action of [FileAction.Probe, FileAction.Transcode]) {
    const url = new URL(withFileAction('https://device.invalid/fs?id=opaque&action=PREVIEW', action))
    expect(url.searchParams.getAll('action')).toEqual([action])
  }
  expect(withFileAction('blob:recording', FileAction.Download)).toBe('blob:recording')
})

it('keeps the action bound to an encrypted managed file URL', () => {
  const url = 'https://device.invalid/proxy/peer/fs?id=opaque&action=PREVIEW'
  expect(withFileAction(url, FileAction.Download)).toBe('https://device.invalid/proxy/peer/fs?id=opaque&action=DOWNLOAD')
})
