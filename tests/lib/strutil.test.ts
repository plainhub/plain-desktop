import { describe, it, expect } from 'vitest'
import { bytesToHex, arrayBufferToHex, addLinksToURLs, getMarkdownTitle } from '@/lib/strutil'

describe('bytesToHex', () => {
  it('encodes empty array', () => {
    expect(bytesToHex(new Uint8Array())).toBe('')
  })

  it('encodes full bytes with leading zero', () => {
    expect(bytesToHex(new Uint8Array([0x00, 0x01, 0x10, 0xff, 0xab]))).toBe('000110ffab')
  })

  it('preserves high nibble (regression: time-sync nonce bug)', () => {
    // (byte & 0x0f).toString(16) dropped the high nibble — e.g.
    // 0xff became "f" instead of "ff". bytesToHex must keep both.
    expect(bytesToHex(new Uint8Array([0xff]))).toBe('ff')
    expect(bytesToHex(new Uint8Array([0x10]))).toBe('10')
    expect(bytesToHex(new Uint8Array([0xab]))).toBe('ab')
  })

  it('encodes 16 random bytes deterministically', () => {
    const arr = Uint8Array.from({ length: 16 }, (_, i) => i * 17)
    // First few bytes: 0*17=0x00, 1*17=0x11, 2*17=0x22, 3*17=0x33, 4*17=0x44...
    expect(bytesToHex(arr)).toBe('00112233445566778899aabbccddeeff')
    expect(bytesToHex(arr)).toHaveLength(32)
  })
})

describe('arrayBufferToHex', () => {
  it('matches bytesToHex over the same bytes', () => {
    const buf = new Uint8Array([0xde, 0xad, 0xbe, 0xef]).buffer
    expect(arrayBufferToHex(buf)).toBe('deadbeef')
  })
})

describe('addLinksToURLs', () => {
  it('does not swallow the trailing semicolon of an HTML entity after a URL', () => {
    // " (U+0022) is HTML-encoded to &#34; by encodeHTML. The URL regex
    // used to match the digits of &#34; as part of the URL and leave a
    // stray ; behind. Regression test for that bug.
    const out = addLinksToURLs('curl "http://mi.local:8080/graphql" -H ok')
    expect(out).toContain('href="http://mi.local:8080/graphql"')
    expect(out).not.toContain('href="http://mi.local:8080/graphql&#')
    expect(out).toContain('&#34;')
    expect(out).toContain(' -H ok')
  })

  it('preserves multi-param query strings', () => {
    const out = addLinksToURLs('see http://example.com?foo=bar&baz=qux ok')
    expect(out).toContain('href="http://example.com?foo=bar&#38;baz=qux"')
    expect(out).toContain(' ok')
  })

  it('leaves URL-adjacent punctuation outside the link', () => {
    const out = addLinksToURLs('check http://example.com/page, then go')
    expect(out).toContain('href="http://example.com/page"')
    expect(out).toContain(', then go')
  })

  it('converts line breaks to <br />', () => {
    const out = addLinksToURLs('line1\nline2')
    expect(out).toBe('line1<br />line2')
  })
})

describe('getMarkdownTitle', () => {
  it('uses the first h1 line as the title', () => {
    expect(getMarkdownTitle('intro\n\n# 发布计划\n\nbody')).toBe('发布计划')
  })

  it('ignores deeper headings and falls back to content', () => {
    expect(getMarkdownTitle('## 待办事项\n\nbody text')).toBe('## 待办事项\n\nbody text'.replace(/\n/g, '').trim().substring(0, 50))
  })

  it('replaces images with a placeholder in the derived title', () => {
    const title = getMarkdownTitle('![shot](app://note-images/a.png) rest')
    expect(title).toBe('🖼 rest')
  })

  it('returns empty string for empty content', () => {
    expect(getMarkdownTitle('')).toBe('')
  })
})
