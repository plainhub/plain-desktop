import { GqlError } from '@/lib/api/gql-error'
import { gqlFetchOp } from './gql-client'
import type { GqlResultType } from './graphql/operations'
import type { HttpServerState } from './local-server'

export type HttpServerSnapshot = GqlResultType<'httpServerChangedSubscription'>['httpServerChanged']

export const restartHttpServerMutation = `mutation($httpPort: Int, $httpsPort: Int) { restartHttpServer(httpPort: $httpPort, httpsPort: $httpsPort) { revision restarting httpPort httpsPort error } }`

export async function restartHttpServer(kind: 'http' | 'https', port: number): Promise<HttpServerState> {
  const result = await gqlFetchOp('restartHttpServerMutation', restartHttpServerMutation, kind === 'http' ? { httpPort: port } : { httpsPort: port })
  if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
  const state = result.data.restartHttpServer
  if (state.restarting) throw new Error('Invalid completed server restart')
  return state
}
