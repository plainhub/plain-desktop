import type { LoginSocket } from './login-socket'

export interface LoginHandshakeResult {
  clientId: string
  token: string
  signaturePublicKey: string
  opaquePublicKey?: string
  chatPaired: boolean
}

interface CompletedLoginResponse {
  clientId: string
  status: 'COMPLETED'
  chatPaired?: boolean
}

export async function receiveLoginResult(
  socket: LoginSocket,
  decryptResponse: (frame: Uint8Array) => string,
  onPending?: () => void
): Promise<CompletedLoginResponse> {
  let response: CompletedLoginResponse | { status: 'PENDING' }
  do {
    response = JSON.parse(decryptResponse(await socket.receiveBytes()))
    if (response.status === 'PENDING') onPending?.()
  } while (response.status === 'PENDING')
  return response
}

export function completeLogin(
  response: CompletedLoginResponse,
  signatureKey: string,
  token: string,
  expectedDeviceId?: string
): LoginHandshakeResult {
  if (response.status !== 'COMPLETED' || typeof response.clientId !== 'string' || !response.clientId) throw 'invalid_auth_response'
  if (expectedDeviceId && response.clientId !== expectedDeviceId) throw 'peer_identity_changed'
  return { clientId: response.clientId, token, signaturePublicKey: signatureKey, chatPaired: response.chatPaired === true }
}
