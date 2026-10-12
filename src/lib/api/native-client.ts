import { invoke } from '@tauri-apps/api/core'

/** The GraphQL request envelope the shared core's executor accepts. */
export interface NativeGraphqlBody {
  query: string
  variables?: Record<string, any>
  operationName?: string
}

/** Runs one GraphQL document through this machine's shared Rust core. Rejects
 *  with a plain `Error`; `gqlFetch` maps it to the shared `GqlError`. */
export async function nativeGraphql<T = any>(body: NativeGraphqlBody, timeout: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined
  const expiry = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error('connection_timeout')), timeout)
  })

  try {
    return (await Promise.race([invoke<T>('native_graphql', { body }), expiry])) as T
  } catch (e: any) {
    if (e?.message === 'connection_timeout') throw e
    throw new Error(typeof e === 'string' ? e : e?.message || 'native_error')
  } finally {
    clearTimeout(timer)
  }
}
