import { ErrorCode } from '@/lib/api/error-code'
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
  if (target.kind === 'peer' && !target.peerId) throw new Error(ErrorCode.NativePeerIdMissing)
  const deadline = Date.now() + timeout
  let timer: ReturnType<typeof setTimeout> | undefined
  const expiry = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(ErrorCode.ConnectionTimeout)), timeout)
  })
  const execute = async () => {
    const scope = await nativeViewScope()
    if (Date.now() >= deadline) throw new Error(ErrorCode.ConnectionTimeout)
    beforeDispatch?.()
    return invoke<T>('native_graphql', { target, body: {
      ...body,
      extensions: { ...body.extensions, nativeScope: scope, nativeDeadlineMs: deadline },
    } })
  }
  try {
    return await Promise.race([execute(), expiry])
  } catch (e: any) {
    if (e?.message === ErrorCode.ConnectionTimeout) throw e
    throw new Error(typeof e === 'string' ? e : e?.message || ErrorCode.NativeError, { cause: e })
  } finally {
    clearTimeout(timer)
  }
}
