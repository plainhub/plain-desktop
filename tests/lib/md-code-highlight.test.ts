import { describe, it, expect } from 'vitest'
import { highlightCode, preloadCodeLanguages } from '@/lib/md-code-highlight'

describe('highlightCode', () => {
  it('escapes code for unknown languages without spans', () => {
    const html = highlightCode('a < b & c', 'notalanguage')
    expect(html).toBe('a &lt; b &amp; c')
    expect(html).not.toContain('<span')
  })

  it('escapes code before any language is preloaded', () => {
    expect(highlightCode('const x', 'js')).toBe('const x')
  })

  it('wraps tokens in md-tok spans after preload', async () => {
    await preloadCodeLanguages('```js\nconst x\n```\n')
    const html = highlightCode('const x = "v"', 'js')
    expect(html).toContain('<span class="md-tok-keyword">const</span>')
    expect(html).toContain('<span class="md-tok-string">"v"</span>')
    expect(html).toContain('</span> x = ')
  })

  it('escapes html-sensitive characters inside tokens', async () => {
    await preloadCodeLanguages('~~~js\n~~~\n')
    const html = highlightCode('const s = "<b>"', 'js')
    expect(html).toContain('&lt;b&gt;')
    expect(html).not.toContain('<b>')
  })

  it('resolves aliases case-insensitively', async () => {
    await preloadCodeLanguages('```TypeScript\n```\n')
    const html = highlightCode('const x', 'TypeScript')
    expect(html).toContain('<span class="md-tok-keyword">const</span>')
  })
})
