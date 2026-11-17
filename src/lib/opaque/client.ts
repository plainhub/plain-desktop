import { AuthErrorCode } from '@/lib/api/auth-error-code'
import { loginErrorReason } from '@/lib/api/login-error'
export interface OpaqueResult {
  ke3: Uint8Array
  apiKey: Uint8Array
  requestAad: Uint8Array
  responseAad: Uint8Array
  serverPublicKey: Uint8Array
}
export type OpaqueCommand =
  | { type: 'START'; password: Uint8Array }
  | { type: 'FINISH'; response: Uint8Array; clientId: string; signingKey: Uint8Array }
export type OpaqueReply =
  | { type: 'REQUEST'; request: Uint8Array }
  | { type: 'RESULT'; result: OpaqueResult }
  | { type: 'ERROR'; reason: AuthErrorCode }

export async function startOpaqueLogin(password: string, signal?: AbortSignal) {
  if (signal?.aborted) throw AuthErrorCode.Canceled
  if (typeof globalThis.crypto?.getRandomValues !== 'function' || typeof Worker !== 'function') {
    throw new DOMException('Secure login unavailable', 'NotSupportedError')
  }
  const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
  let failure: AuthErrorCode | undefined
  let pending: { resolve: (value: OpaqueReply) => void; reject: (reason: AuthErrorCode) => void } | undefined
  const close = (reason: AuthErrorCode) => {
    if (failure) return
    failure = reason
    clearTimeout(deadline)
    signal?.removeEventListener('abort', abort)
    pending?.reject(reason)
    pending = undefined
    worker.terminate()
  }
  const abort = () => close(AuthErrorCode.Canceled)
  const deadline = setTimeout(() => close(AuthErrorCode.Timeout), 300000)
  worker.onmessage = (event: MessageEvent<OpaqueReply>) => {
    if (event.data.type === 'ERROR') { close(event.data.reason); return }
    pending?.resolve(event.data)
    pending = undefined
  }
  worker.onerror = () => close(AuthErrorCode.InternalError)
  signal?.addEventListener('abort', abort, { once: true })
  const exchange = (command: OpaqueCommand): Promise<OpaqueReply> => new Promise((resolve, reject) => {
    if (failure) { reject(failure); return }
    pending = { resolve, reject }
    try { worker.postMessage(command) }
    catch { close(AuthErrorCode.InternalError) }
  })
  const bytes = new TextEncoder().encode(password)
  try {
    const starting = exchange({ type: 'START', password: bytes })
    bytes.fill(0)
    const hello = await starting
    if (hello.type !== 'REQUEST') throw AuthErrorCode.InternalError
    let finished = false
    return {
      request: () => hello.request,
      async finish(response: Uint8Array, clientId: string, signingKey: Uint8Array): Promise<OpaqueResult> {
        if (finished) throw AuthErrorCode.InternalError
        finished = true
        try {
          const reply = await exchange({ type: 'FINISH', response, clientId, signingKey })
          if (reply.type !== 'RESULT') throw AuthErrorCode.InternalError
          return reply.result
        } finally { close(AuthErrorCode.Canceled) }
      },
      free: () => close(AuthErrorCode.Canceled),
    }
  } catch (error) {
    close(loginErrorReason(error))
    throw error
  } finally { bytes.fill(0) }
}
