import { loginErrorReason } from '@/lib/api/login-error'
import { ErrorCode } from '@/lib/api/error-code'
export interface OpaqueResult {
  ke3: Uint8Array
  apiKey: Uint8Array
  requestAad: Uint8Array
  responseAad: Uint8Array
  serverPublicKey: Uint8Array
}
export type OpaqueCommand =
  | { type: 'start'; password: Uint8Array }
  | { type: 'finish'; response: Uint8Array; clientId: string; signingKey: Uint8Array }
export type OpaqueReply =
  | { type: 'request'; request: Uint8Array }
  | { type: 'result'; result: OpaqueResult }
  | { type: 'error'; reason: ErrorCode }

export async function startOpaqueLogin(password: string, signal?: AbortSignal) {
  if (signal?.aborted) throw ErrorCode.Canceled
  if (typeof globalThis.crypto?.getRandomValues !== 'function' || typeof Worker !== 'function') throw ErrorCode.SecureLoginUnavailable
  const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
  let failure: ErrorCode | undefined
  let pending: { resolve: (value: OpaqueReply) => void; reject: (reason: ErrorCode) => void } | undefined
  const close = (reason: ErrorCode) => {
    if (failure) return
    failure = reason
    clearTimeout(deadline)
    signal?.removeEventListener('abort', abort)
    pending?.reject(reason)
    pending = undefined
    worker.terminate()
  }
  const abort = () => close(ErrorCode.Canceled)
  const deadline = setTimeout(() => close(ErrorCode.LoginExpired), 300000)
  worker.onmessage = (event: MessageEvent<OpaqueReply>) => {
    if (event.data.type === 'error') { close(event.data.reason); return }
    pending?.resolve(event.data)
    pending = undefined
  }
  worker.onerror = () => close(ErrorCode.InvalidAuthResponse)
  signal?.addEventListener('abort', abort, { once: true })
  const exchange = (command: OpaqueCommand): Promise<OpaqueReply> => new Promise((resolve, reject) => {
    if (failure) { reject(failure); return }
    pending = { resolve, reject }
    try { worker.postMessage(command) }
    catch { close(ErrorCode.InvalidAuthResponse) }
  })
  const bytes = new TextEncoder().encode(password)
  try {
    const starting = exchange({ type: 'start', password: bytes })
    bytes.fill(0)
    const hello = await starting
    if (hello.type !== 'request') throw ErrorCode.InvalidAuthResponse
    let finished = false
    return {
      request: () => hello.request,
      async finish(response: Uint8Array, clientId: string, signingKey: Uint8Array): Promise<OpaqueResult> {
        if (finished) throw ErrorCode.InvalidAuthResponse
        finished = true
        try {
          const reply = await exchange({ type: 'finish', response, clientId, signingKey })
          if (reply.type !== 'result') throw ErrorCode.InvalidAuthResponse
          return reply.result
        } finally { close(ErrorCode.Canceled) }
      },
      free: () => close(ErrorCode.Canceled),
    }
  } catch (error) {
    close(loginErrorReason(error))
    throw error
  } finally { bytes.fill(0) }
}
