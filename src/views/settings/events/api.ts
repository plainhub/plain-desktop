import { gqlFetch } from '@/lib/api/gql-client'
import type { GqlResultType } from '@/lib/api/graphql/operations'

export const eventsGQL = `query events($query: String!, $offset: Int!, $limit: Int!) {
  events(query: $query, offset: $offset, limit: $limit) { id type message clientId createdAt }
}`

export type EventRecord = GqlResultType<'eventsGQL'>['events'][number]

export async function readEvents(text: string, offset: number, limit: number) {
  const result = await gqlFetch<{ events: EventRecord[] }>(eventsGQL, {
    query: text.trim() ? `text:${JSON.stringify(text.trim())}` : '', offset, limit,
  }, { fresh: true })
  return result.data!.events
}
