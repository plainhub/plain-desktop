import type { IChatItem } from '../interfaces'
import type { GqlVariablesType } from '../api/graphql/operations'

export function chatContentInput(content: IChatItem['content']): GqlVariablesType<'sendChatItemsGQL'>['content'] {
  switch (content.__typename) {
    case 'ChatTextContent': return { text: { text: content.text, linkPreviews: content.linkPreviews } }
    case 'ChatImagesContent': return { images: { items: content.items.map(({ id, uri, size, durationMs, width, height, summary, fileName }) => ({ id, uri, size, durationMs, width, height, summary, fileName })) } }
    case 'ChatFilesContent': return { files: { items: content.items.map(({ id, uri, size, durationMs, width, height, summary, fileName }) => ({ id, uri, size, durationMs, width, height, summary, fileName })) } }
    case 'ChatShareContent': return { share: { shareId: content.shareId, urlToken: content.urlToken, peerInfo: content.peerInfo, name: content.name, itemCount: content.itemCount, totalSize: content.totalSize, expiresAt: content.expiresAt } }
  }
}
