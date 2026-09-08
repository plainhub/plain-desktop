import { describe, expect, it } from 'vitest'
import { generateFileId } from '@/lib/upload/upload'

// Different content in files that share their 16-char name prefix and have
// equal metadata string lengths — the exact shape that used to collide when
// crypto.subtle was unavailable (plain HTTP) and the fallback hash only mixed
// the buffer length plus the first 16 bytes (GH plainhub/plain-app#358).
function makeFile(name: string, size: number, mtime: number, fill: number): File {
  const body = new Uint8Array(size).fill(fill)
  return new File([body], name, { lastModified: mtime })
}

describe('generateFileId', () => {
  it('distinguishes files sharing a name prefix and metadata length', async () => {
    const a = makeFile('20260704_083339.jpg', 3 * 1024 * 1024, 1753622003000, 0x11)
    const b = makeFile('20260704_083339.jpg', 3 * 1024 * 1024, 1753622003000, 0x22)
    expect(await generateFileId(a)).not.toBe(await generateFileId(b))
  })

  it('is stable for the same file (resume depends on it)', async () => {
    const file = makeFile('video.mp4', 5 * 1024 * 1024, 1753622003999, 0xab)
    expect(await generateFileId(file)).toBe(await generateFileId(file))
  })

  it('distinguishes metadata that differs only outside the name prefix', async () => {
    const a = makeFile('20260704_083339.jpg', 3 * 1024 * 1024, 1753622003000, 0x11)
    const b = makeFile('20260704_083339.jpg', 4 * 1024 * 1024, 1753622003000, 0x11)
    const c = makeFile('20260704_083339.jpg', 3 * 1024 * 1024, 1753622003999, 0x11)
    const ids = new Set([await generateFileId(a), await generateFileId(b), await generateFileId(c)])
    expect(ids.size).toBe(3)
  })
})
