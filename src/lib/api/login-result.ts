import { computeECDHSharedKey, verifyEd25519Signature } from './crypto'
import { getSyncedTimestamp } from './time-sync'
import { base64ToArrayBuffer } from '@/lib/strutil'
import type { LoginSocket } from './login-socket'

export interface LoginHandshakeResult {
  clientId: string
  token: string
  signaturePublicKey: string
  chatPaired: boolean
}

interface CompletedLoginResponse {
  clientId: string
  status: 'COMPLETED'
  ecdhPublicKey: string
  timestamp: number
  chatPaired?: boolean
  loginContext: string
  signature: string
}

export async function receiveLoginResult(
  socket: LoginSocket,
  decryptResponse: (frame: Uint8Array) => string,
  onPending?: () => void
): Promise<CompletedLoginResponse> {
  let response: CompletedLoginResponse | { status: 'PENDING' }
  do {
    const frame = await socket.receiveBytes()
    const plaintext = decryptResponse(frame)
    response = JSON.parse(plaintext)
    if (response.status === 'PENDING') onPending?.()
  } while (response.status === 'PENDING')
  return response
}

export function completeLogin(
  response: CompletedLoginResponse,
  signatureKey: string,
  secretKey: Uint8Array,
  expectedDeviceId: string | undefined,
  context: string
): LoginHandshakeResult {
  if (
    response.status !== 'COMPLETED' ||
    !Number.isSafeInteger(response.timestamp) ||
    Math.abs(getSyncedTimestamp() - response.timestamp) > 300000
  ) {
    throw 'signature_verification_failed'
  }
  if (!response.clientId || (expectedDeviceId && response.clientId !== expectedDeviceId)) {
    throw 'peer_identity_changed'
  }
  if (response.loginContext !== context) throw 'invalid_auth_response'

  const signedFields = [
    String(response.clientId),
    String(response.status),
    String(response.ecdhPublicKey),
    String(response.timestamp),
  ]
  if (response.chatPaired) signedFields.push('true')
  signedFields.push(context)
  if (!verifyEd25519Signature(signatureKey, signedFields.join('|'), response.signature)) {
    throw 'signature_verification_failed'
  }

  const serverPublicKey = base64ToArrayBuffer(response.ecdhPublicKey)
  return {
    clientId: response.clientId,
    token: computeECDHSharedKey(secretKey, serverPublicKey),
    signaturePublicKey: signatureKey,
    chatPaired: response.chatPaired === true,
  }
}
