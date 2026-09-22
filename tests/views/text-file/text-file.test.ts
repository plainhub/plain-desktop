import { describe, expect, it } from 'vitest'
import { fileNameFromDecryptedPath } from '@/views/text-file/text-file'

describe('fileNameFromDecryptedPath', () => {
  it('extracts the basename from a plain posix path', () => {
    expect(fileNameFromDecryptedPath('/storage/emulated/0/Download/notes.txt')).toBe('notes.txt')
  })

  it('extracts the basename from a windows path', () => {
    expect(fileNameFromDecryptedPath('C:\\Users\\me\\doc.md')).toBe('doc.md')
  })

  it('returns the file name for a bare relative path', () => {
    expect(fileNameFromDecryptedPath('readme.md')).toBe('readme.md')
  })

  it('returns empty for non-path decrypted payloads', () => {
    expect(fileNameFromDecryptedPath('')).toBe('')
    expect(fileNameFromDecryptedPath('fid:abc123.txt')).toBe('')
    expect(fileNameFromDecryptedPath('app://files/notes.txt')).toBe('')
    expect(fileNameFromDecryptedPath('https://example.com/a.txt')).toBe('')
    expect(fileNameFromDecryptedPath('{"path":"/a/b.txt","mediaId":"m1"}')).toBe('')
  })

  it('ignores a trailing separator', () => {
    expect(fileNameFromDecryptedPath('/storage/notes/')).toBe('notes')
  })
})
