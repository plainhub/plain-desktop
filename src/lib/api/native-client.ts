import { invoke } from '@/lib/api/native-invoke'

export interface NativeGraphqlBody {
  query: string
  variables?: Record<string, any>
  operationName?: string
  extensions?: Record<string, unknown>
}

export type NativeTarget = { kind: 'local' } | { kind: 'peer'; peerId: string }

let documentScope: Promise<string> | undefined

export function nativeViewScope(): Promise<string> {
  return documentScope ??= invoke<string>('native_view_scope')
}

export async function nativeGraphql<T = any>(body: NativeGraphqlBody, timeout: number, target: NativeTarget = { kind: 'local' }, beforeDispatch?: () => void): Promise<T> {
  if (target.kind === 'peer' && !target.peerId) throw new Error('native_peer_id_missing')
  const deadline = Date.now() + timeout
  let timer: ReturnType<typeof setTimeout> | undefined
  const expiry = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error('connection_timeout')), timeout)
  })
  const execute = async () => {
    const scope = await nativeViewScope()
    if (Date.now() >= deadline) throw new Error('connection_timeout')
    beforeDispatch?.()
    return invoke<T>('native_graphql', { target, body: {
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
