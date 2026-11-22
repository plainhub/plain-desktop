import { expect, it, vi } from 'vitest'
import { h } from 'vue'
import { DataType } from '@/lib/data'
import type { ISms } from '@/lib/interfaces'
import { createPendingMms } from '@/lib/message-helpers'
import { generateRecordId } from '@/lib/record-id.generated'
import { addPendingMms, addPendingSms, settlePendingSmsResult } from '@/lib/sms-state-sync'
import MessageChatBubble from '@/views/messages/MessageChatBubble.vue'
import MessageChatList from '@/views/messages/MessageChatList.vue'
import VCircularProgress from '@/components/base/VCircularProgress.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import { click, clientTemplate, content, nodes, renderView } from '../helpers/vue-node-renderer'

vi.mock('@/lib/format', () => ({ formatDateTime: () => 'November 22', formatTime: () => '19:53' }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

clientTemplate(MessageChatList, 'src/views/messages/MessageChatList.vue')
clientTemplate(MessageChatBubble, 'src/views/messages/MessageChatBubble.vue')
clientTemplate(VIconButton, 'src/components/base/VIconButton.vue')

const TagRelationsDropdown = {
  props: ['item'],
  setup: (props: { item: { key: string } }) => () => h('button', { 'data-tag-for': props.item.key }),
}

const message: ISms = {
  id: 'fixture-message',
  threadId: 'fixture-thread',
  type: 'INBOX',
  body: 'Test message',
  address: '5550100',
  serviceCenter: '',
  sentAt: '2026-11-22T11:53:00Z',
  subscriptionId: 0,
  tags: [],
}

function renderMessages(items: ISms[], onTrash = vi.fn()) {
  return renderView(MessageChatList, {
    items,
    onTrash,
    listError: '',
    detailLoading: false,
    loading: false,
    loadingMore: false,
    access: [],
    urlTokenKey: null,
    tags: [],
    type: DataType.SMS,
  }, { TagRelationsDropdown, VCircularProgress, VIconButton })
}

it.each(['INBOX', 'SENT'] as const)('puts the %s actions in order on the timestamp row below the bubble', (type) => {
  const item = { ...message, type }
  const onTrash = vi.fn()
  const view = renderMessages([item], onTrash)
  try {
    const elements = nodes(view.root)
    const bubble = elements.find(node => node.props.class === 'chat-bubble')!
    const time = elements.find(node => content(node) === '19:53' && node.tag === 'span')!
    const tag = elements.find(node => node.props['data-tag-for'] === item.id)!
    const trash = elements.find(node =>
      node.tag === 'button' && node.children.some(child => child.tag === 'i-material-symbols:delete-outline-rounded'),
    )!
    expect(time.parent!.children.filter(node => node.tag[0] !== '#').map(node => elements.indexOf(node)))
      .toEqual([time, tag, trash].map(node => elements.indexOf(node)))
    expect(bubble.parent).toBe(time.parent!.parent)
    expect(bubble.parent!.children.indexOf(bubble)).toBeLessThan(bubble.parent!.children.indexOf(time.parent!))
    click(trash)
    expect(onTrash).toHaveBeenCalledExactlyOnceWith(item)
  } finally {
    view.unmount()
  }
})

const smsRequestId = generateRecordId()
const pendingSms = addPendingSms([], smsRequestId, message.body, message.address, message.threadId)
const sentSms = settlePendingSmsResult(pendingSms, { requestId: smsRequestId, success: true, resultCode: -1 }).pending
const mms = createPendingMms(generateRecordId(), '', message.address, message.threadId, [
  { path: 'data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==', contentType: 'image/gif', name: 'pixel.gif' },
])

it.each([
  ['pending text message', pendingSms, 'sending'],
  ['acknowledged text message', sentSms, 'sent'],
  ['pending multimedia message', addPendingMms([], mms), 'sending'],
  ['expired multimedia message', addPendingMms([], { ...mms, sentAt: new Date(Date.now() - 301_000).toISOString() }), 'mms_canceled'],
  ['saved draft', [{ ...message, type: 'DRAFT' }] satisfies ISms[], 'message_type.DRAFT'],
] as const)('shows the correct status and no actions for the %s', (_name, items, expected) => {
  const view = renderMessages(items)
  try {
    const elements = nodes(view.root)
    const status = elements.find(node => String(node.props.class).startsWith('chat-pending-status'))
    expect(status && content(status).trim()).toBe(expected)
    expect(elements.some(node => node.tag === 'button')).toBe(false)
  } finally {
    view.unmount()
  }
})
