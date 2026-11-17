import { NativeErrorCode } from '@/lib/api/native-error-code'
import { PeerConnectionErrorCode } from '@/lib/api/peer-connection-error-code'
import { invoke } from '@/lib/api/native-invoke'

export interface NativeGraphqlBody {
  query: string
  variables?: Record<string, any>
  operationName?: string
  extensions?: Record<string, unknown>
}

export type NativeTarget = { kind: 'LOCAL' } | { kind: 'PEER'; peerId: string }

let documentScope: Promise<string> | undefined

export function nativeViewScope(): Promise<string> {
  return documentScope ??= invoke<string>('native_view_scope')
}

export async function nativeGraphql<T = any>(body: NativeGraphqlBody, timeout: number, target: NativeTarget = { kind: 'LOCAL' }, beforeDispatch?: () => void): Promise<T> {
  if (target.kind === 'PEER' && !target.peerId) throw new Error(NativeErrorCode.NativePeerIdMissing)
  const deadline = Date.now() + timeout
  let timer: ReturnType<typeof setTimeout> | undefined
  const expiry = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(PeerConnectionErrorCode.ConnectionTimeout)), timeout)
  })
  const execute = async () => {
    const scope = await nativeViewScope()
    if (Date.now() >= deadline) throw new Error(PeerConnectionErrorCode.ConnectionTimeout)
    beforeDispatch?.()
    return invoke<T>('native_graphql', { target, body: {
      ...body,
      extensions: { ...body.extensions, nativeScope: scope, nativeDeadlineMs: deadline },
    } })
  }
  try {
    return await Promise.race([execute(), expiry])
  } catch (e: any) {
    if (e?.message === PeerConnectionErrorCode.ConnectionTimeout) throw e
    throw new Error(typeof e === 'string' ? e : e?.message || NativeErrorCode.NativeError, { cause: e })
  } finally {
    clearTimeout(timer)
  }
}
