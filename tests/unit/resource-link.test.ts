import { expect, it, vi } from 'vitest'
import { xchacha20poly1305 } from '@noble/ciphers/chacha'
import { encryptResourcePayload, resourceId } from '@/lib/api/resource-link'

vi.mock('@noble/ciphers/webcrypto', () => ({ randomBytes: (length: number) => Uint8Array.from({ length }, (_, index) => index) }))

it('uses a single base64url payload and binds the complete variable client identifier', () => {
  const key = new Uint8Array(32).fill(7)
  const clientId = '客户.a/b'
  const resource = { type: 'FILE' as const, id: '/photos/a.jpg' }
  const envelope = resourceId(key, { mode: 'c', id: clientId }, resource)
  expect(envelope.startsWith(`c.${clientId}.`)).toBe(true)
  const payload = envelope.slice(envelope.lastIndexOf('.') + 1)
  expect(payload).toMatch(/^[\w-]+$/)
  const binary = Buffer.from(payload, 'base64url')
  expect(binary.length).toBe(24 + new TextEncoder().encode(JSON.stringify(resource)).length + 16)
  const aad = new TextEncoder().encode(`plain-fs-v1\0c\0${clientId}`)
  expect(new TextDecoder().decode(xchacha20poly1305(key, binary.subarray(0, 24), aad).decrypt(binary.subarray(24)))).toBe(JSON.stringify(resource))
  expect(() => xchacha20poly1305(key, binary.subarray(0, 24), new TextEncoder().encode('plain-fs-v1\0c\0another')).decrypt(binary.subarray(24))).toThrow()
  expect(envelope).toBe('c.客户.a/b.AAECAwQFBgcICQoLDA0ODxAREhMUFRYX0zXf_Ip5eZ8HgcgLo8CDjjayGi022EcZnGnY8rWtuv1zWa0R2rowTfBttQ9uc8GObm-lNQ')
})

it('keeps session, global, share and endpoint contexts separate', () => {
  const key = new Uint8Array(32).fill(7)
  const global = resourceId(key, { mode: 'g' }, { type: 'FILE', id: '/photos/a.jpg' })
  expect(global.startsWith('g.')).toBe(true)
  const share = resourceId(key, { mode: 's', id: 'share.with.dots' }, { type: 'SHARED_FILE', id: '旅行/照片.jpg' })
  expect(share.startsWith('s.share.with.dots.')).toBe(true)
  expect(() => resourceId(key, { mode: 'g' }, { type: 'SHARED_FILE', id: 'file' })).toThrow()
  expect(() => resourceId(key, { mode: 's', id: 'share' }, { type: 'FILE', id: '/file' })).toThrow()
  expect(encryptResourcePayload(key, { mode: 'g' }, 'proxyfs', '/file')).not.toBe(encryptResourcePayload(key, { mode: 'g' }, 'zip-files', '/file'))
})
