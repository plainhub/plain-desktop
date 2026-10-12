import { describe, it, expect, vi, beforeEach } from 'vitest'

const nativeResourceUrl = vi.fn()
vi.mock('@/hooks/native-resource-url', () => ({
  nativeResourceUrl: (...args: unknown[]) => nativeResourceUrl(...args),
}))

import {
  notId,
  getFileUrl,
  getFileName,
  tokenToKey,
  encryptUrlParams,
  getPeerProxyUrl,
  getFinalPath,
  resolveFileUrlByPath,
  download,
} from '@/lib/api/file'
import { hashToKey } from '@/lib/api/crypto'

describe('notId', () => {
  it('returns true for http:// URL', () => {
    expect(notId('http://example.com/file.txt')).toBe(true)
  })

  it('returns true for https:// URL', () => {
    expect(notId('https://cdn.example.com/img.png')).toBe(true)
  })

  it('returns true for blob: URL', () => {
    expect(notId('blob:http://localhost/abc-123')).toBe(true)
  })

  it('returns false for a plain file ID', () => {
    expect(notId('abc123def456')).toBe(false)
  })

  it('returns false for a path-like ID', () => {
    expect(notId('/data/photos/image.jpg')).toBe(false)
  })

  it('is case-insensitive for HTTP prefix', () => {
    expect(notId('HTTP://EXAMPLE.COM')).toBe(true)
    expect(notId('HTTPS://EXAMPLE.COM')).toBe(true)
  })
})

describe('getFileUrl', () => {
  it('returns the ID as-is when it is an http URL', () => {
    const url = 'http://cdn.example.com/file.jpg'
    expect(getFileUrl(url)).toBe(url)
  })

  it('returns the ID as-is when it is an https URL', () => {
    const url = 'https://example.com/photo.png'
    expect(getFileUrl(url)).toBe(url)
  })

  it('returns the ID as-is when it is a blob URL', () => {
    const url = 'blob:http://localhost/uuid-value'
    expect(getFileUrl(url)).toBe(url)
  })

  it('builds /fs?id= URL for a plain ID', () => {
    const result = getFileUrl('myfileId123')
    expect(result).toMatch(/\/fs\?id=myfileId123$/)
  })

  it('URL-encodes the ID in the query string', () => {
    const result = getFileUrl('file id with space')
    expect(result).toContain('file%20id%20with%20space')
  })

  it('appends extra query string when provided', () => {
    const result = getFileUrl('myId', '&size=thumb')
    expect(result).toMatch(/&size=thumb$/)
  })

  it('uses empty query string by default', () => {
    const result = getFileUrl('myId')
    expect(result).not.toContain('&')
    expect(result).toMatch(/\/fs\?id=myId$/)
  })
})

describe('getFileName', () => {
  it('extracts filename from a path', () => {
    expect(getFileName('/photos/2024/image.jpg')).toBe('image.jpg')
  })

  it('returns the name without directory for simple filename', () => {
    expect(getFileName('README.md')).toBe('README.md')
  })

  it('returns empty string for null', () => {
    expect(getFileName(null as any)).toBe('')
  })

  it('returns empty string for undefined', () => {
    expect(getFileName(undefined as any)).toBe('')
  })

  it('returns empty string for empty string', () => {
    expect(getFileName('')).toBe('')
  })

  it('handles trailing slash (empty filename)', () => {
    expect(getFileName('/path/to/dir/')).toBe('')
  })

  it('handles deeply nested path', () => {
    expect(getFileName('/a/b/c/d/e/file.txt')).toBe('file.txt')
  })
})

describe('tokenToKey', () => {
  it('converts base64 token to Uint8Array', () => {
    const token = btoa('hello world')
    const key = tokenToKey(token)
    expect(key).toBeInstanceOf(Uint8Array)
    expect(key.length).toBe('hello world'.length)
  })

  it('each byte matches the ASCII code of the decoded character', () => {
    const token = btoa('ABC')
    const key = tokenToKey(token)
    expect(key[0]).toBe('A'.charCodeAt(0))
    expect(key[1]).toBe('B'.charCodeAt(0))
    expect(key[2]).toBe('C'.charCodeAt(0))
  })

  it('handles 32-byte key encoded as base64', () => {
    const raw = new Uint8Array(32).fill(0xff)
    const token = btoa(String.fromCharCode(...raw))
    const key = tokenToKey(token)
    expect(key).toHaveLength(32)
    expect(key.every((b) => b === 0xff)).toBe(true)
  })
})

describe('encryptUrlParams', () => {
  it('returns empty string when key is null', () => {
    expect(encryptUrlParams(null, 'some=params')).toBe('')
  })

  it('returns a non-empty base64 string when key is provided', () => {
    const key = hashToKey('a'.repeat(64))
    const result = encryptUrlParams(key, 'id=123&size=thumb')
    expect(result).toBeTruthy()
    expect(typeof result).toBe('string')
    // Should be base64
    expect(() => atob(result)).not.toThrow()
  })

  it('different params produce different ciphertext', () => {
    const key = hashToKey('a'.repeat(64))
    const r1 = encryptUrlParams(key, 'params=1')
    const r2 = encryptUrlParams(key, 'params=2')
    expect(r1).not.toBe(r2)
  })
})

describe('getPeerProxyUrl', () => {
  const key = hashToKey('a'.repeat(64))
  const peer = { ip: '192.168.1.5', port: 8443 }

  it('returns empty string when key is null', () => {
    expect(getPeerProxyUrl(null, peer, 'fileId')).toBe('')
  })

  it('returns empty string when peer ip is missing', () => {
    expect(getPeerProxyUrl(key, { ip: '', port: 8443 }, 'fileId')).toBe('')
  })

  it('returns empty string when peerFileId is empty', () => {
    expect(getPeerProxyUrl(key, peer, '')).toBe('')
  })

  it('returns a /proxyfs?id= URL when all params valid', () => {
    const result = getPeerProxyUrl(key, peer, 'encFileId123')
    expect(result).toMatch(/\/proxyfs\?id=/)
  })
})

describe('getFinalPath', () => {
  it('resolves app:// paths relative to appDir', () => {
    expect(getFinalPath('/home/user/app', 'app://data/file.txt')).toBe('/home/user/app/data/file.txt')
  })

  it('resolves fid: paths into a sharded structure', () => {
    const result = getFinalPath('/home/user/app', 'fid:abcd1234')
    // Should be /home/user/app/ab/cd/abcd1234
    expect(result).toBe('/home/user/app/ab/cd/abcd1234')
  })

  it('returns path unchanged for plain paths', () => {
    expect(getFinalPath('/home/user/app', '/absolute/path/file.txt')).toBe('/absolute/path/file.txt')
  })

  it('handles app:// with empty sub-path', () => {
    const result = getFinalPath('/appDir', 'app://')
    expect(result).toBe('/appDir/')
  })
})

describe('download', () => {
  it('prevents duplicate downloads within 1 second', () => {
    const fakeLink = { download: '', href: '', click: vi.fn() }
    const appendSpy = vi.spyOn(document.body, 'appendChild').mockReturnValue(fakeLink as any)
    const removeSpy = vi.spyOn(document.body, 'removeChild').mockReturnValue(fakeLink as any)
    vi.spyOn(document, 'createElement').mockReturnValue(fakeLink as any)

    const url = 'http://example.com/unique-file-' + Date.now()
    download(url, 'test.zip')
    download(url, 'test.zip') // duplicate — should be skipped

    // createElement/appendChild should only be called once
    expect(appendSpy).toHaveBeenCalledTimes(1)

    appendSpy.mockRestore()
    removeSpy.mockRestore()
    vi.restoreAllMocks()
  })
})

describe('resolveFileUrlByPath', () => {
  const key = new Uint8Array([1, 2, 3, 4])

  beforeEach(() => {
    nativeResourceUrl.mockReset()
  })

  it('returns the native resource url when this machine owns the file', async () => {
    nativeResourceUrl.mockResolvedValue('plain-resource://file/h1')
    await expect(resolveFileUrlByPath(key, '/data/a.jpg')).resolves.toBe('plain-resource://file/h1')
    expect(nativeResourceUrl).toHaveBeenCalledWith('/data/a.jpg', undefined)
  })

  it('passes the size through so oversized files keep their existing transport', async () => {
    nativeResourceUrl.mockResolvedValue('')
    await resolveFileUrlByPath(key, '/data/big.mp4', 1024)
    expect(nativeResourceUrl).toHaveBeenCalledWith('/data/big.mp4', 1024)
  })

  it('falls back to the existing /fs url for remote targets', async () => {
    nativeResourceUrl.mockResolvedValue('')
    await expect(resolveFileUrlByPath(key, '/data/a.jpg')).resolves.toMatch(/\/fs\?id=/)
  })

  it('resolves a core reference without a session key', async () => {
    nativeResourceUrl.mockResolvedValue('plain-resource://file/h1')
    await expect(resolveFileUrlByPath(null, 'fid:abc123.jpg')).resolves.toBe('plain-resource://file/h1')
    expect(nativeResourceUrl).toHaveBeenCalledWith('fid:abc123.jpg', undefined)
  })

  it('keeps the thumbnail query on the session transport', async () => {
    nativeResourceUrl.mockResolvedValue('')
    await expect(resolveFileUrlByPath(key, '/data/a.jpg', undefined, '&w=512&h=512')).resolves.toMatch(/w=512&h=512/)
  })

  it('returns empty without a native handle, a key or a path', async () => {
    nativeResourceUrl.mockResolvedValue('')
    await expect(resolveFileUrlByPath(null, '/data/a.jpg')).resolves.toBe('')
    await expect(resolveFileUrlByPath(key, '')).resolves.toBe('')
    expect(nativeResourceUrl).toHaveBeenCalledTimes(1)
  })
})
