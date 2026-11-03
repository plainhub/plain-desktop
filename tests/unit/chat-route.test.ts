import { expect, it, vi } from 'vitest'

vi.mock('@/stores/main', () => ({ useMainStore: vi.fn() }))
vi.mock('@/stores/temp', () => ({ useTempStore: vi.fn() }))
vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
import { decryptChatId, encryptChatId } from '@/views/chat/hooks/chat-route'

it.each(['channel:test-channel', 'peer:test-peer', 'channel:中文频道'])('opens the requested chat from its generated route: %s', id => {
  const key = new Uint8Array(32).fill(7)
  const route = encryptChatId(id, key)
  expect(decryptChatId(decodeURIComponent(encodeURIComponent(route)), key)).toBe(id)
})

it('does not treat missing, malformed or wrong-key route IDs as another chat', () => {
  const key = new Uint8Array(32).fill(7)
  expect(encryptChatId('channel:test', null)).toBe('')
  for (const id of ['', 'not-base64']) expect(decryptChatId(id, key)).toBe('peer:local')
  expect(decryptChatId(encryptChatId('channel:test', key), new Uint8Array(32).fill(8))).toBe('peer:local')
  expect(decryptChatId(encryptChatId('/files', key), key)).toBe('peer:local')
})
