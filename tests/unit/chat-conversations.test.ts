import { expect, it, vi } from 'vitest'
import { ref } from 'vue'

const api = vi.hoisted(() => ({ receive: undefined as undefined | ((data: any) => void), fetch: vi.fn() }))
vi.mock('@/lib/api/query', () => ({
  chatConversationsGQL: 'conversation-snapshot',
  initLazyQuery: ({ handle, document }: any) => {
    expect(document).toBe('conversation-snapshot')
    api.receive = handle
    return { fetch: api.fetch, loading: false }
  },
}))
vi.mock('@/lib/prefs', () => ({ get: () => 'self' }))
vi.mock('@/lib/chat/peer-cacher', () => ({ peerCacher: { peersMap: ref({}) } }))
vi.mock('@/lib/chat/channel-cacher', () => ({ channelCacher: { channelsMap: ref({}) } }))

import { chatCacher } from '@/lib/chat/chat-cacher'

it('projects Rust conversation IDs without requiring a second peer or channel snapshot', async () => {
  api.receive!({ chatConversations: [
    { id: 'peer', chat: { id: 'latest', content: '{"type":"TEXT","value":{"text":"reply"}}', fromId: 'peer', toId: 'me', channelId: null } },
    { id: 'group', chat: { id: 'group-latest', content: '{}', channelId: 'group' } },
    { id: 'local', chat: { id: 'local-latest', content: '{}', channelId: null } },
  ] })
  expect(Object.keys(chatCacher.latestChatMap.value)).toEqual(['peer:peer', 'channel:group', 'peer:local'])
  expect(chatCacher.getLatestChat('peer:peer')?.id).toBe('latest')
  await chatCacher.load()
  expect(api.fetch).toHaveBeenCalledTimes(1)
  api.receive!({ chatConversations: [] })
  expect(chatCacher.latestChatMap.value).toEqual({})
})
