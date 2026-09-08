import { gqlFetch } from '@/lib/api/gql-client'
import { chatItemGQL, chatItemsGQL } from '@/lib/api/query'

// Single-item query first (local server); device servers (plain-app) only
// expose the conversation query, so degrade to it when the single query
// yields no data. Returns null when neither path resolves fresh content.
export async function fetchLatestChatContent(messageId: string, conversationId: string): Promise<string | null> {
  try {
    const single = await gqlFetch(chatItemGQL, { id: messageId })
    const fresh = single.data?.chatItem?.content
    if (typeof fresh === 'string') return fresh
    const list = await gqlFetch(chatItemsGQL, { id: conversationId })
    return list.data?.chatItems?.find((i: any) => i.id === messageId)?.content ?? null
  } catch {
    return null
  }
}
