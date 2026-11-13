import { loginErrorReason } from './login-error'
import { startOpaqueLogin } from '@/lib/opaque/client'
import { base64ToArrayBuffer } from '@/lib/strutil'
import { bitArrayToBase64, chachaDecryptBytes, chachaEncryptBytes } from './crypto'
import { openLoginSocket, type LoginSocket } from './login-socket'
import { completeLogin, receiveLoginResult } from './login-result'
import type { LoginHandshakeParams, LoginHandshakeResult } from './login-handshake'

const utf8 = new TextEncoder()

function publicKey(value: string | undefined): Uint8Array {
  if (!value) throw 'invalid_auth_response'
  const bytes = base64ToArrayBuffer(value)
  if (bytes.length !== 32 || bitArrayToBase64(bytes) !== value) throw 'invalid_auth_response'
  return bytes
}

export async function performOpaqueLogin(
  params: LoginHandshakeParams,
  signatureKey: string,
  url: string,
  metadata: () => Promise<object>
): Promise<LoginHandshakeResult> {
  if (params.signal?.aborted) throw 'cancelled'
  const signingKey = publicKey(signatureKey)
  const pinnedKey = publicKey(params.initOpaquePublicKey)
  if (params.storedOpaquePublicKey && params.storedOpaquePublicKey !== params.initOpaquePublicKey) {
    throw 'peer_identity_changed'
  }
  let login: Awaited<ReturnType<typeof startOpaqueLogin>> | undefined
  let socket: LoginSocket | undefined
  let key: Uint8Array | undefined
  try {
    login = await startOpaqueLogin(params.password!, params.signal)
    if (params.signal?.aborted) throw 'cancelled'
    socket = await openLoginSocket(url, params.signal)
    socket.send(login.request())
    const response = await socket.receiveBytes()
    const active = login
    login = undefined // finish consumes the WASM state even when authentication fails.
    const result = await active.finish(response, params.clientId, signingKey)
    key = result.apiKey
    if (bitArrayToBase64(result.serverPublicKey) !== bitArrayToBase64(pinnedKey)) throw 'peer_identity_changed'
    const { ke3, requestAad, responseAad } = result
    const details = await socket.waitFor(metadata())
    const encrypted = chachaEncryptBytes(key, utf8.encode(JSON.stringify(details)), requestAad)
    const request = new Uint8Array(ke3.length + encrypted.length)
    request.set(ke3)
    request.set(encrypted, ke3.length)
    socket.send(request)
    const apiKey = key
    const reply = await receiveLoginResult(socket,
      frame => new TextDecoder().decode(chachaDecryptBytes(apiKey, frame, responseAad)),
      () => params.onPending?.())
    return { ...completeLogin(reply, signatureKey, bitArrayToBase64(key), params.expectedDeviceId), opaquePublicKey: params.initOpaquePublicKey }

  } catch (error) {
    const reason = loginErrorReason(error)
    socket?.close(reason)
    throw reason
  } finally {
    login?.free()
    key?.fill(0)
    socket?.close()
  }
}
