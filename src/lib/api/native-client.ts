import { invoke } from '@tauri-apps/api/core'

export interface NativeGraphqlBody {
  query: string
  variables?: Record<string, any>
  operationName?: string
  extensions?: Record<string, unknown>
}

let documentScope: Promise<string> | undefined

export async function nativeGraphql<T = any>(body: NativeGraphqlBody, timeout: number): Promise<T> {
  const deadline = Date.now() + timeout
  let timer: ReturnType<typeof setTimeout> | undefined
  const expiry = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error('connection_timeout')), timeout)
  })
  const execute = async () => {
    documentScope ??= invoke<string>('native_view_scope')
    const scope = await documentScope
    if (Date.now() >= deadline) throw new Error('connection_timeout')
    return invoke<T>('native_graphql', { body: {
      ...body,
      extensions: { ...body.extensions, nativeScope: scope, nativeDeadlineMs: deadline },
    } })
  }
  try {
    return await Promise.race([execute(), expiry])
  } catch (e: any) {
    if (e?.message === 'connection_timeout') throw e
    throw new Error(typeof e === 'string' ? e : e?.message || 'native_error', { cause: e })
  } finally {
    clearTimeout(timer)
  }
}
