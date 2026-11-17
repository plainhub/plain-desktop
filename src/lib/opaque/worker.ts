import { AuthErrorCode, parseAuthErrorCode } from '@/lib/api/auth-error-code'
import init, { OpaqueLogin } from './pkg/plain_auth'
import type { OpaqueCommand, OpaqueReply } from './client'

let login: OpaqueLogin | undefined
self.onmessage = async (event: MessageEvent<OpaqueCommand>) => {
  const command = event.data
  try {
    if (command.type === 'START') {
      try {
        if (typeof globalThis.crypto?.getRandomValues !== 'function') throw AuthErrorCode.InternalError
        await init()
        login = OpaqueLogin.start(command.password)
      } finally { command.password.fill(0) }
      self.postMessage({ type: 'REQUEST', request: login.request() } satisfies OpaqueReply)
    } else {
      if (!login) throw AuthErrorCode.InternalError
      const active = login
      login = undefined // finish consumes the WASM state, including failures.
      const result = active.finish(command.response, command.clientId, command.signingKey)
      try {
        const message = { type: 'RESULT', result: {
          ke3: result.ke3,
          apiKey: result.apiKey,
          requestAad: result.requestAad,
          responseAad: result.responseAad,
          serverPublicKey: result.serverPublicKey,
        } } satisfies OpaqueReply
        try { self.postMessage(message) }
        finally { message.result.apiKey.fill(0) }
      } finally { result.free() }
    }
  } catch (error) {
    login?.free()
    login = undefined
    self.postMessage({ type: 'ERROR', reason: parseAuthErrorCode(error) ?? AuthErrorCode.InternalError } satisfies OpaqueReply)
  }
}
