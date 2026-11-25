import { computed, h } from 'vue'
import { expect, it, vi } from 'vitest'
import { useChatMessages } from '@/views/chat/hooks/chat-messages'
import { flush, renderView } from '../helpers/vue-node-renderer'

const fetch = vi.hoisted(() => vi.fn())
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: fetch }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { onAttached: () => () => {} } }))
vi.mock('@/lib/api/mutation', () => ({
  sendChatItemsGQL: 'send',
  deleteChatItemGQL: 'delete',
  retryChatItemGQL: 'retry',
  initMutation: () => ({ mutate: vi.fn(), onDone: vi.fn() }),
}))
vi.mock('@/views/chat/hooks/chat-subscriptions', () => ({
  normalizeChatItem: (item: unknown) => item,
  useChatSubscriptions: vi.fn(),
}))
vi.mock('@/views/chat/hooks/chat', () => ({ useTasks: () => ({ cancel: vi.fn() }) }))
vi.mock('@/hooks/show-action-error', () => ({ showActionError: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

it('requests the newest chat page and displays it oldest first without changing the response', async () => {
  const rows = [{ id: 'new' }, { id: 'old' }]
  fetch.mockResolvedValue({ data: { chatItems: rows } })
  let model!: ReturnType<typeof useChatMessages>
  const view = renderView({ setup() {
    model = useChatMessages(computed(() => 'peer:local'), computed(() => ''))
    return () => h('section')
  } })
  try {
    await flush()
    expect(fetch).toHaveBeenCalledWith(
      expect.stringMatching(/chatItems\(query: \$query, offset: 0, limit: 200, sortBy: NEWEST_FIRST\)/),
      { query: 'peer:local' },
      { fresh: false },
    )
    expect(model.chatItems.value.map(item => item.id)).toEqual(['old', 'new'])
    expect(rows.map(item => item.id)).toEqual(['new', 'old'])
  } finally {
    view.unmount()
  }
})
