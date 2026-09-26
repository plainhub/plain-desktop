import { describe, expect, it, vi } from 'vitest'
import { DeviceType } from '@/lib/status'

const socket = vi.hoisted(() => ({ close: vi.fn(), send: vi.fn(), readyState: 0, onopen: null as null | (() => Promise<void>), onmessage: null as null | ((event: MessageEvent) => Promise<void>) }))
const openSocket = vi.hoisted(() => vi.fn(() => socket))

vi.mock('@/lib/api/http', () => ({ openSocket }))
vi.mock('@/lib/api/api', () => ({ getWebSocketBaseUrl: () => 'wss://device.test' }))
vi.mock('@/lib/api/crypto', () => ({
  generateECDHKeyPair: () => ({ publicKey: new Uint8Array(65), secretKey: new Uint8Array(32) }),
  bitArrayToBase64: () => '',
  bitArrayToUint8Array: (value: Uint8Array) => value,
  chachaEncrypt: vi.fn((_key: Uint8Array, plaintext: string) => new TextEncoder().encode(plaintext)),
  chachaDecrypt: vi.fn(),
  computeECDHSharedKey: vi.fn(),
  verifyEd25519Signature: vi.fn(),
}))
vi.mock('@/lib/agent/agent', () => ({ getAccurateAgent: vi.fn() }))
vi.mock('@/lib/api/time-sync', () => ({ getSyncedTimestamp: () => Date.now() }))

import { performLoginHandshake } from '@/lib/api/login-handshake'
import { chachaDecrypt, computeECDHSharedKey, verifyEd25519Signature } from '@/lib/api/crypto'
import { getAccurateAgent } from '@/lib/agent/agent'

describe('login handshake cancellation', () => {
  it('closes the pending socket when the login form is dismissed', async () => {
    openSocket.mockClear()
    socket.close.mockClear()
    const controller = new AbortController()
    const result = performLoginHandshake({ passwordHash: 'a'.repeat(128), clientId: 'desktop', signal: controller.signal })
    expect(openSocket).toHaveBeenCalledOnce()
    controller.abort()
    await expect(result).rejects.toBe('cancelled')
    expect(socket.close).toHaveBeenCalledWith(3001, 'cancelled')
  })

  it('does not open a socket for an already cancelled login', async () => {
    openSocket.mockClear()
    const controller = new AbortController()
    controller.abort()
    await expect(performLoginHandshake({ passwordHash: 'a'.repeat(128), clientId: 'desktop', signal: controller.signal })).rejects.toBe('cancelled')
    expect(openSocket).not.toHaveBeenCalled()
  })

  it('sends peer metadata and verifies the signed chat pairing result', async () => {
    socket.send.mockClear()
    vi.mocked(getAccurateAgent).mockResolvedValue({ browser: { name: 'Chrome', version: '1' }, os: { name: 'macOS', version: '1' }, isMobile: false } as Awaited<ReturnType<typeof getAccurateAgent>>)
    vi.mocked(computeECDHSharedKey).mockReturnValue('AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=')
    vi.mocked(verifyEd25519Signature).mockReturnValue(true)
    const peer = { deviceName: 'Mac', port: 8443, deviceType: DeviceType.COMPUTER, ips: ['192.0.2.2'], signaturePublicKey: 'desktop-key' }
    const result = performLoginHandshake({ passwordHash: 'a'.repeat(128), clientId: 'desktop', initSignaturePublicKey: 'phone-key', peer })
    await socket.onopen?.()
    const request = JSON.parse(new TextDecoder().decode(socket.send.mock.calls[0][0]))
    expect(request.peer).toEqual(peer)

    vi.mocked(chachaDecrypt).mockReturnValue(JSON.stringify({ clientId: 'phone', status: 'COMPLETED', ecdhPublicKey: btoa('phone-key'), timestamp: Date.now(), signature: 'signature', chatPaired: true }))
    await socket.onmessage?.({ data: { arrayBuffer: async () => new ArrayBuffer(0) } } as MessageEvent)
    await expect(result).resolves.toMatchObject({ clientId: 'phone', chatPaired: true })
    expect(vi.mocked(verifyEd25519Signature).mock.calls.at(-1)?.[1]).toMatch(/\|true$/)
  })
})
