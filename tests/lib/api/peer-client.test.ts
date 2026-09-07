import { describe, it, expect, vi, afterEach } from 'vitest'
import { gqlFetchPeer } from '@/lib/api/peer-client'
import { GqlError } from '@/lib/api/gql-client'
import { chachaEncrypt, chachaDecrypt, arrayBufferToBitArray } from '@/lib/api/crypto'
import { tokenToKey } from '@/lib/api/file'
import type { LoginPeer } from '@/lib/device/login-peers'

// Only ip / port / token are read by gqlFetchPeer; a *43 TLS port keeps
// deviceBaseUrl on https like a real phone.
const peer = {
  id: 'peer1',
  ip: '192.168.1.5',
  port: 8443,
  token: btoa('a'.repeat(64)),
} as unknown as LoginPeer
const key = tokenToKey(peer.token)

function encryptedBody(data: object): ArrayBuffer {
  const encrypted = chachaEncrypt(key, JSON.stringify(data))
  return encrypted.buffer.slice(encrypted.byteOffset, encrypted.byteOffset + encrypted.byteLength) as ArrayBuffer
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('gqlFetchPeer', () => {
  it('POSTs to the peer server over https', async () => {
    const fetchMock = vi.fn(async () => ({ status: 200, arrayBuffer: async () => encryptedBody({ data: { ok: 1 } }) }))
    vi.stubGlobal('fetch', fetchMock)

    await gqlFetchPeer(peer, 'query { ok }')
    expect(fetchMock.mock.calls[0][0]).toBe('https://192.168.1.5:8443/graphql')
    expect(fetchMock.mock.calls[0][1].method).toBe('POST')
  })

  it('round-trips the encrypted payload under the peer token', async () => {
    const fetchMock = vi.fn(async () => ({ status: 200, arrayBuffer: async () => encryptedBody({ data: { notifications: [] } }) }))
    vi.stubGlobal('fetch', fetchMock)

    const result = await gqlFetchPeer(peer, 'query { notifications }', { limit: 10 })

    const sent: Uint8Array = fetchMock.mock.calls[0][1].body
    const decrypted = chachaDecrypt(key, arrayBufferToBitArray(sent.buffer.slice(sent.byteOffset, sent.byteOffset + sent.byteLength) as ArrayBuffer))
    // Replay protection wraps the JSON as "ts|nonce|json" — keep everything after the second pipe.
    const payload = JSON.parse(decrypted.split('|').slice(2).join('|'))
    expect(payload).toEqual({ query: 'query { notifications }', variables: { limit: 10 } })

    expect(result.data).toEqual({ notifications: [] })
  })

  it('throws GqlError unauthorized on 401', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ({ status: 401, arrayBuffer: async () => new ArrayBuffer(0) })))

    const err = await gqlFetchPeer(peer, 'query { x }').catch((e) => e)
    expect(err).toBeInstanceOf(GqlError)
    expect(err).toMatchObject({ message: 'unauthorized', status: 401 })
  })
})
