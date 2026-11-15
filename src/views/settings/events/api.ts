import { gqlFetch } from '@/lib/api/gql-client'
import type { GqlResultType } from '@/lib/api/graphql/operations'

export const eventsGQL = `query events($query: String!, $offset: Int!, $limit: Int!) {
  events(query: $query, offset: $offset, limit: $limit) {
    id type clientId clientName clientIp osName osVersion appName appVersion createdAt
    data {
      __typename
      ... on LoginEventData { sessionType }
      ... on LoginFailedEventData { reason }
      ... on SessionEventData { session { clientId clientName type } }
      ... on DeviceNameEventData { oldName newName }
      ... on VolumeEventData {
        volume { id name devicePath mountPath fileSystem }
      }
      ... on VolumeFailedEventData {
        volume { id name devicePath mountPath fileSystem }
        failure { reason detail }
      }
      ... on DiskFormatEventData {
        disk { id name path sizeBytes }
        fileSystem
      }
      ... on DiskFormatFailedEventData {
        disk { id name path sizeBytes }
        fileSystem
        failure { reason detail }
      }
    }
  }
}`

export type EventRecord = GqlResultType<'eventsGQL'>['events'][number]

export async function readEvents(text: string, offset: number, limit: number) {
  const result = await gqlFetch<{ events: EventRecord[] }>(eventsGQL, {
    query: text.trim() ? `text:${JSON.stringify(text.trim())}` : '', offset, limit,
  }, { fresh: true })
  if (result.errors?.length) throw new Error(result.errors[0].message)
  return result.data.events
}
