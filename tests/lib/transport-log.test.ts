import { describe, it, expect, vi, afterEach } from 'vitest'
import { gqlFetch } from '@/lib/api/gql-client'

// gqlFetch logs the encrypted-outgoing request body before touching the
// network, so a rejecting fetch is enough to exercise the gate.
vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('network off')))

const infoSpy = vi.spyOn(console, 'info').mockImplementation(() => {})

afterEach(() => {
  delete window.__PLAIN_LOG__
  infoSpy.mockClear()
})

describe('transport logging opt-in flag', () => {
  it('logs nothing by default (flag undefined)', async () => {
    await gqlFetch('query { x }', { a: 1 }).catch(() => {})
    expect(infoSpy).not.toHaveBeenCalled()
  })

  it('emits [request] when window.__PLAIN_LOG__ is true', async () => {
    window.__PLAIN_LOG__ = true
    await gqlFetch('query { x }', { a: 1 }).catch(() => {})
    expect(infoSpy).toHaveBeenCalledTimes(1)
    expect(String(infoSpy.mock.calls[0][0])).toContain('[request]')
    expect(String(infoSpy.mock.calls[0][0])).toContain('query { x }')
  })

  it('stops logging as soon as the flag is cleared', async () => {
    window.__PLAIN_LOG__ = true
    await gqlFetch('query { x }').catch(() => {})
    expect(infoSpy).toHaveBeenCalledTimes(1)
    delete window.__PLAIN_LOG__
    await gqlFetch('query { y }').catch(() => {})
    expect(infoSpy).toHaveBeenCalledTimes(1)
  })
})
