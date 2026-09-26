import { describe, it, expect, vi, afterEach } from 'vitest'
import { gqlFetch } from '@/lib/api/gql-client'

// gqlFetch logs the encrypted-outgoing request body before touching the
// network, so a rejecting fetch is enough to exercise the gate.
vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('network off')))

const infoSpy = vi.spyOn(console, 'info').mockImplementation(() => {})
const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

afterEach(() => {
  delete window.__PLAIN_LOG__
  infoSpy.mockClear()
  warnSpy.mockClear()
})

describe('transport logging opt-in flag', () => {
  it('logs nothing by default (flag undefined)', async () => {
    await gqlFetch('query { x }', { a: 1 }).catch(() => {})
    expect(infoSpy).not.toHaveBeenCalled()
    expect(warnSpy).not.toHaveBeenCalled()
  })

  it('emits one [gql] → line with the full pre-encryption body when window.__PLAIN_LOG__ is true', async () => {
    window.__PLAIN_LOG__ = true
    await gqlFetch('query { x }', { a: 1 }).catch(() => {})
    expect(infoSpy).toHaveBeenCalledTimes(1)
    expect(String(infoSpy.mock.calls[0][0])).toContain('[gql] →')
    expect(String(infoSpy.mock.calls[0][0])).toContain('query { x }')
    // The failed call also logs one ✗ line (warn, not info).
    expect(warnSpy).toHaveBeenCalledTimes(1)
    expect(String(warnSpy.mock.calls[0][0])).toContain('[gql] ✗')
  })

  it('stops logging as soon as the flag is cleared', async () => {
    window.__PLAIN_LOG__ = true
    await gqlFetch('query { x }').catch(() => {})
    expect(infoSpy).toHaveBeenCalledTimes(1)
    delete window.__PLAIN_LOG__
    await gqlFetch('query { y }').catch(() => {})
    expect(infoSpy).toHaveBeenCalledTimes(1)
    expect(warnSpy).toHaveBeenCalledTimes(1)
  })
})

describe('[gql] operation label', () => {
  it('names an anonymous query after its first top-level field', async () => {
    window.__PLAIN_LOG__ = true
    await gqlFetch('query {\n  app {\n    ...AppFragment\n  }\n}\nfragment AppFragment on App {\n  clientId\n}').catch(() => {})
    expect(String(infoSpy.mock.calls[0][0])).toContain('[gql] → app @')
  })

  it('skips variable definitions of an anonymous operation', async () => {
    window.__PLAIN_LOG__ = true
    await gqlFetch('query ($id: ID!) { item(id: $id) { name } }').catch(() => {})
    expect(String(infoSpy.mock.calls[0][0])).toContain('[gql] → item @')
  })

  it('names an anonymous mutation after its first top-level field', async () => {
    window.__PLAIN_LOG__ = true
    await gqlFetch('mutation { sendChatItem(text: "hi") { id } }').catch(() => {})
    expect(String(infoSpy.mock.calls[0][0])).toContain('[gql] → sendChatItem @')
  })

  it('names an anonymous spread-only operation after the fragment', async () => {
    window.__PLAIN_LOG__ = true
    await gqlFetch('query { ...AppFragment }').catch(() => {})
    expect(String(infoSpy.mock.calls[0][0])).toContain('[gql] → AppFragment @')
  })

  it('names a shorthand document ({ field }) after its first field', async () => {
    window.__PLAIN_LOG__ = true
    await gqlFetch('{ app { id } }').catch(() => {})
    expect(String(infoSpy.mock.calls[0][0])).toContain('[gql] → app @')
  })

  it('keeps using the declared operation name when present', async () => {
    window.__PLAIN_LOG__ = true
    await gqlFetch('query AppInfo { app { id } }').catch(() => {})
    expect(String(infoSpy.mock.calls[0][0])).toContain('[gql] → AppInfo @')
  })
})
