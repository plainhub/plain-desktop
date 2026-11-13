import { loginErrorReason } from './login-error'
import { getWebSocketBaseUrl } from '@/lib/api/api'
import { getAccurateAgent } from '@/lib/agent/agent'
import { openLoginSocket } from './login-socket'
import type { LoginSocket } from './login-socket'
import { startLoginKeyExchange } from './login-channel'
import type { LoginKeys } from './login-channel'
import { completeLogin, receiveLoginResult } from './login-result'
import type { LoginHandshakeResult } from './login-result'
import type { DeviceType } from '@/lib/status'

export type { LoginHandshakeResult } from './login-result'

export interface LoginHandshakeParams {
  passwordRequired: boolean
  password?: string
  initOpaquePublicKey?: string
  storedOpaquePublicKey?: string
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

async function createLoginMetadata(params: LoginHandshakeParams) {
  const agent = await getAccurateAgent()
  return {
    appName: agent.browser.name,
    appVersion: agent.browser.version,
    osName: agent.os.name,
    osVersion: agent.os.version,
    ...(params.peer ? { peer: params.peer } : {}),
  }
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
  return url.toString()
}

async function exchangeLoginKeys(
  socket: LoginSocket,
  params: LoginHandshakeParams,
  signatureKey: string
): Promise<LoginKeys> {
  const exchange = startLoginKeyExchange(params.clientId)
  try {
    socket.send(JSON.stringify(exchange.hello))
    const hello = await socket.receiveText()
    return exchange.accept(JSON.parse(hello), signatureKey, params.expectedDeviceId)
  } finally { exchange.dispose() }
}

export async function performLoginHandshake(
  params: LoginHandshakeParams
): Promise<LoginHandshakeResult> {
  if (params.signal?.aborted) throw 'cancelled'
  if (params.passwordRequired && !params.password) throw 'invalid_password'
  const signatureKey = verifyDeviceIdentity(params)
  if (params.passwordRequired) {
    const { performOpaqueLogin } = await import('./opaque-login')
    return performOpaqueLogin(params, signatureKey, getLoginUrl(params), () => createLoginMetadata(params))
  }
  const socket = await openLoginSocket(getLoginUrl(params), params.signal)

  let keys: LoginKeys | undefined
  try {
    keys = await exchangeLoginKeys(socket, params, signatureKey)
    const channel = keys
    const metadata = await socket.waitFor(createLoginMetadata(params))
    const request = JSON.stringify(metadata)
    socket.send(keys.encryptRequest(new TextEncoder().encode(request)))
    const response = await receiveLoginResult(
      socket,
      (frame) => new TextDecoder().decode(channel.decryptResponse(frame)),
      () => params.onPending?.(channel.verificationCode)
    )
    const result = completeLogin(
      response,
      signatureKey,
      keys.token,
      keys.deviceId
    )
    socket.close()
    return result
  } catch (error) {
    const reason = loginErrorReason(error)
    socket.close(reason)
    throw reason
  } finally { keys?.dispose() }
}
