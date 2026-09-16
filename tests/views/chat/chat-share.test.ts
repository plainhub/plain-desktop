import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import ChatShare from '@/views/chat/ChatShare.vue'
import { MessageType } from '@/lib/status'

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/lib/browser', () => ({ openUrl: vi.fn() }))

import { openUrl } from '@/lib/browser'

const FAR_FUTURE = '2999-01-01T00:00:00Z'
const PAST = '2000-01-01T00:00:00Z'

function makeItem(value: Record<string, any>) {
  return {
    id: 'm1',
    fromId: 'p1',
    toId: '',
    channelId: '',
    createdAt: '2026-09-16T00:00:00Z',
    content: JSON.stringify({ type: MessageType.SHARE, value }),
    _content: { type: MessageType.SHARE, value },
    __typename: 'ChatItem',
    data: { ids: [] },
  }
}

const baseShare = {
  shareId: 'abc123',
  urlToken: 'tok-xyz',
  peerInfo: { id: 'peer-1', ip: '192.168.1.8', port: 8443 },
  name: 'Holiday photos',
  itemCount: 12,
  totalSize: 1536,
  expiresAt: FAR_FUTURE,
}

function mountShare(value: Record<string, any>) {
  return mount(ChatShare, { props: { data: makeItem(value) } })
}

const wrappers: Array<ReturnType<typeof mount>> = []
afterEach(() => {
  while (wrappers.length) wrappers.pop()!.unmount()
  vi.mocked(openUrl).mockClear()
})

describe('ChatShare', () => {
  it('renders name and the count · size · expiry subtitle', () => {
    const w = mountShare(baseShare)
    wrappers.push(w)
    expect(w.find('.share-name').text()).toBe('Holiday photos')
    expect(w.find('.meta-text').text()).toBe('folder_card_items · 1.5 kB · share_expires_on')
    expect(w.find('.expired-badge').exists()).toBe(false)
  })

  it('omits size when zero and expiry when absent', () => {
    const w = mountShare({ ...baseShare, totalSize: 0, expiresAt: null })
    wrappers.push(w)
    expect(w.find('.meta-text').text()).toBe('folder_card_items')
  })

  it('opens the /s/<id>#<token> page on the sender endpoint when clicked', async () => {
    const w = mountShare(baseShare)
    wrappers.push(w)
    await w.find('.share-card').trigger('click')
    expect(openUrl).toHaveBeenCalledWith('https://192.168.1.8:8443/s/abc123#tok-xyz')
  })

  it('does not open anything when share fields are missing', async () => {
    const w = mountShare({ ...baseShare, shareId: '', urlToken: '' })
    wrappers.push(w)
    await w.find('.share-card').trigger('click')
    expect(openUrl).not.toHaveBeenCalled()
  })

  it('marks expired shares with a badge, dims the card and drops the expiry part', () => {
    const w = mountShare({ ...baseShare, expiresAt: PAST })
    wrappers.push(w)
    expect(w.find('.share-card').classes()).toContain('expired')
    expect(w.find('.expired-badge').text()).toBe('share_expired')
    expect(w.find('.meta-text').text()).toBe('folder_card_items · 1.5 kB')
  })
})
