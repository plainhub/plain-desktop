import { bitArrayToBase64, generateECDHKeyPair } from '@/lib/api/crypto'
import { getWebSocketBaseUrl } from '@/lib/api/api'
import { getAccurateAgent } from '@/lib/agent/agent'
import { openLoginSocket } from './login-socket'
import type { LoginSocket } from './login-socket'
import { startLoginKeyExchange } from './login-channel'
import type { LoginKeys } from './login-channel'
import { createPasswordEncryption } from './password-login'
import { completeLogin, receiveLoginResult } from './login-result'
import type { LoginHandshakeResult } from './login-result'
import type { DeviceType } from '@/lib/status'
import type { ECDHKeyPair } from './crypto'

export type { LoginHandshakeResult } from './login-result'

export interface LoginHandshakeParams {
  passwordRequired: boolean
  passwordHash?: string
  clientId: string
  storedSignaturePublicKey?: string
  initSignaturePublicKey?: string
  expectedDeviceId?: string
  onPending?: (verificationCode?: string) => void
  signal?: AbortSignal
  peer?: {
    deviceName: string
    port: number
    deviceType: DeviceType
    ips: string[]
    signaturePublicKey: string
  }
}

async function createLoginRequest(
  params: LoginHandshakeParams,
  session: ECDHKeyPair,
  keys: LoginKeys,
  password?: string
) {
  const agent = await getAccurateAgent()
  return JSON.stringify({
    browserName: agent.browser.name,
    browserVersion: agent.browser.version,
    osName: agent.os.name,
    osVersion: agent.os.version,
    isMobile: agent.isMobile,
    ecdhPublicKey: bitArrayToBase64(session.publicKey),
    loginContext: keys.context,
    ...(password !== undefined ? { password } : {}),
    ...(params.peer ? { peer: params.peer } : {}),
  })
}

async function passwordLogin(
  socket: LoginSocket,
  params: LoginHandshakeParams,
  session: ECDHKeyPair,
  keys: LoginKeys
) {
  const encryption = createPasswordEncryption(keys, params.passwordHash!)
  const request = await socket.waitFor(
    createLoginRequest(params, session, keys, params.passwordHash)
  )
  socket.send(encryption.encryptRequest(request))

  return receiveLoginResult(socket, encryption.decryptResponse, () => params.onPending?.())
}

async function deviceConfirmationLogin(
  socket: LoginSocket,
  params: LoginHandshakeParams,
  session: ECDHKeyPair,
  keys: LoginKeys
) {
  const request = await socket.waitFor(createLoginRequest(params, session, keys))
  const bytes = new TextEncoder().encode(request)
  socket.send(keys.encryptRequest(bytes))

  return receiveLoginResult(
    socket,
    (frame) => new TextDecoder().decode(keys.decryptResponse(frame)),
    () => params.onPending?.(keys.verificationCode)
  )
}

function verifyDeviceIdentity(params: LoginHandshakeParams): string {
  if (
    params.storedSignaturePublicKey &&
    params.initSignaturePublicKey &&
    params.storedSignaturePublicKey !== params.initSignaturePublicKey
  ) {
    throw 'peer_identity_changed'
  }
  const signatureKey = params.storedSignaturePublicKey || params.initSignaturePublicKey
  if (!signatureKey) throw 'signature_verification_failed'

  return signatureKey
}

function getLoginUrl(params: LoginHandshakeParams): string {
  const url = new URL('/auth', getWebSocketBaseUrl())
  url.searchParams.set('cid', params.clientId)
  url.searchParams.set('auth', params.passwordRequired ? '1' : '2')
  return url.toString()
}

async function exchangeLoginKeys(
  socket: LoginSocket,
  params: LoginHandshakeParams,
  signatureKey: string
): Promise<LoginKeys> {
  const mode = params.passwordRequired ? 'password' : 'deviceConfirmation'
  const exchange = startLoginKeyExchange(params.clientId, mode)
  socket.send(JSON.stringify(exchange.hello))

  const hello = await socket.receiveText()
  const keys = exchange.accept(JSON.parse(hello), signatureKey, params.expectedDeviceId)
  socket.send(JSON.stringify(keys.reveal))
  return keys
}

export async function performLoginHandshake(
  params: LoginHandshakeParams
): Promise<LoginHandshakeResult> {
  if (params.signal?.aborted) throw 'cancelled'
  if (params.passwordRequired && !params.passwordHash) throw 'invalid_password'
  const signatureKey = verifyDeviceIdentity(params)
  const session = generateECDHKeyPair()
  const socket = await openLoginSocket(getLoginUrl(params), params.signal)

  try {
    const keys = await exchangeLoginKeys(socket, params, signatureKey)

    let response
    if (params.passwordRequired) {
      response = await passwordLogin(socket, params, session, keys)
    } else {
      response = await deviceConfirmationLogin(socket, params, session, keys)
    }
    const result = completeLogin(
      response,
      signatureKey,
      session.secretKey,
      params.expectedDeviceId,
      keys.context
    )
    socket.close()
    return result
  } catch (error) {
    const reason = typeof error === 'string' ? error : 'invalid_auth_response'
    socket.close(reason)
    throw reason
  }
}
