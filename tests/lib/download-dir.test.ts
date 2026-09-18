import { describe, expect, it } from 'vitest'
import { joinPath, uniqueDownloadPath } from '@/lib/download-dir'

describe('joinPath', () => {
  it('joins dir and name with a single slash', () => {
    expect(joinPath('/Users/mac/Downloads', 'a.jpg')).toBe('/Users/mac/Downloads/a.jpg')
    expect(joinPath('/Users/mac/Downloads/', 'a.jpg')).toBe('/Users/mac/Downloads/a.jpg')
    expect(joinPath('/Users/mac/Downloads//', 'a.jpg')).toBe('/Users/mac/Downloads/a.jpg')
  })

  it('keeps windows separators trimmed', () => {
    expect(joinPath('C:\\Users\\mac\\Downloads\\', 'a.jpg')).toBe('C:\\Users\\mac\\Downloads/a.jpg')
  })
})

describe('uniqueDownloadPath', () => {
  it('returns the plain path when it does not exist', async () => {
    const path = await uniqueDownloadPath('/dl', 'a.jpg', async () => false)
    expect(path).toBe('/dl/a.jpg')
  })

  it('appends an increasing counter before the extension on collisions', async () => {
    const taken = new Set(['/dl/a.jpg', '/dl/a (2).jpg'])
    const path = await uniqueDownloadPath('/dl', 'a.jpg', async (p) => taken.has(p))
    expect(path).toBe('/dl/a (3).jpg')
  })

  it('handles names without extension', async () => {
    const taken = new Set(['/dl/README'])
    const path = await uniqueDownloadPath('/dl', 'README', async (p) => taken.has(p))
    expect(path).toBe('/dl/README (2)')
  })
})
