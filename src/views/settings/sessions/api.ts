import { GqlError } from '@/lib/api/gql-error'
import { gqlFetch } from '@/lib/api/gql-client'
import type { GqlResultType } from '@/lib/api/graphql/operations'

export const sessionsGQL = `query sessions {
  sessions { clientId clientName type clientIp osName osVersion appName appVersion createdAt lastSeenAt }
}`

export const renameSessionGQL = `mutation renameSession($clientId: ID!, $name: String!) {
  renameSession(clientId: $clientId, name: $name)
}`

export const revokeSessionGQL = `mutation revokeSession($clientId: ID!) {
  revokeSession(clientId: $clientId)
}`

export const createApiTokenGQL = `mutation createApiToken($name: String!) {
  createApiToken(name: $name) { clientId token }
}`

export type CreatedApiToken = GqlResultType<'createApiTokenGQL'>['createApiToken']

export type SessionRecord = GqlResultType<'sessionsGQL'>['sessions'][number]

export async function createApiToken(name: string) {
  const result = await gqlFetch<{ createApiToken: CreatedApiToken }>(createApiTokenGQL, { name })
  if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
  return result.data.createApiToken
}

export async function readSessions() {
  const result = await gqlFetch<{ sessions: SessionRecord[] }>(sessionsGQL, {}, { fresh: true })
  if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
  return result.data.sessions
}

export async function renameSession(clientId: string, name: string) {
  const result = await gqlFetch<{ renameSession: boolean }>(renameSessionGQL, { clientId, name })
  if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
  return result.data.renameSession
}

export async function revokeSession(clientId: string) {
  const result = await gqlFetch<{ revokeSession: boolean }>(revokeSessionGQL, { clientId })
  if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
  return result.data.revokeSession
}
