import { gqlFetchOp } from './gql-client'
import type { GqlResultType } from './graphql/operations'
import type { HttpServerState } from './local-server'

export type HttpServerSnapshot = GqlResultType<'httpServerQuery'>['httpServer']

export const httpServerQuery = `query {
  httpServer {
    state { revision restarting httpPort httpsPort error }
    addresses
    onlineClients
    remoteAccess
    mdnsHostname
  }
}`
export const restartHttpServerMutation = `mutation($httpPort: Int, $httpsPort: Int) { restartHttpServer(httpPort: $httpPort, httpsPort: $httpsPort) { revision restarting httpPort httpsPort error } }`

export async function readHttpServer(): Promise<HttpServerSnapshot> {
  const result = await gqlFetchOp('httpServerQuery', httpServerQuery, undefined, { fresh: true })
  if (result.errors?.length) throw new Error(result.errors[0].message)
  return result.data.httpServer
}

export async function restartHttpServer(kind: 'http' | 'https', port: number): Promise<HttpServerState> {
  const result = await gqlFetchOp('restartHttpServerMutation', restartHttpServerMutation, kind === 'http' ? { httpPort: port } : { httpsPort: port })
  if (result.errors?.length) throw new Error(result.errors[0].message)
  const state = result.data.restartHttpServer
  if (state.restarting) throw new Error('Invalid completed server restart')
  return state
}
