import type { IChatItem } from '../interfaces'
import type { ChatAttachmentType } from './message-draft'
import type { GqlVariablesType } from '../api/graphql/operations'

export function chatContentInput(content: IChatItem['content']): GqlVariablesType<'sendChatItemsGQL'>['content'] {
  switch (content.__typename) {
    case 'ChatMessageContent': return { message: {
      text: content.text, format: content.format,
      items: content.items.map(({ type, attachment: { id, uri, size, durationMs, width, height, summary, fileName } }) => ({
        type, attachment: { id, uri, size, durationMs, width, height, summary, fileName },
      })),
    } }
    case 'ChatShareContent': return { share: { shareId: content.shareId, urlToken: content.urlToken, peerInfo: content.peerInfo, name: content.name, itemCount: content.itemCount, totalSize: content.totalSize, expiresAt: content.expiresAt } }
  }
}

export function chatAttachments(content: IChatItem['content'] | undefined, type: ChatAttachmentType) {
  return content?.__typename === 'ChatMessageContent'
    ? content.items.filter(item => item.type === type).map(item => item.attachment)
    : []
}
