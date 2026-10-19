import { chachaEncrypt, chachaDecrypt, bitArrayToUint8Array, bitArrayToBase64, generateECDHKeyPair, computeECDHSharedKey, verifyEd25519Signature } from '@/lib/api/crypto'
import { getWebSocketBaseUrl } from '@/lib/api/api'
import { getAccurateAgent } from '@/lib/agent/agent'
import { openSocket } from '@/lib/api/http'
import { getSyncedTimestamp } from '@/lib/api/time-sync'
import { startLoginChannel } from './login-channel'
import type { DeviceType } from '@/lib/status'

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

export interface LoginHandshakeResult {
  clientId: string
  token: string
  signaturePublicKey: string
  chatPaired: boolean
}

function loopback(host: string) {
  return ['localhost', '127.0.0.1', '[::1]'].includes(host)
}

export function secureLoginTransport(): boolean {
  const base = new URL(getWebSocketBaseUrl())
  return (window.location.protocol === 'https:' || loopback(window.location.hostname)) && (base.protocol === 'wss:' || loopback(base.hostname))
}

export function performLoginHandshake(params: LoginHandshakeParams): Promise<LoginHandshakeResult> {
  if (params.signal?.aborted) return Promise.reject('cancelled')
  if (params.storedSignaturePublicKey && params.initSignaturePublicKey && params.storedSignaturePublicKey !== params.initSignaturePublicKey) return Promise.reject('peer_identity_changed')
  const signatureKey = params.storedSignaturePublicKey || params.initSignaturePublicKey
  if (!signatureKey) return Promise.reject('signature_verification_failed')
  const base = new URL(getWebSocketBaseUrl())
  if (!secureLoginTransport()) return Promise.reject('secure_connection_required')
  if (params.passwordRequired && !params.passwordHash) return Promise.reject('invalid_password')
  const passwordKey = params.passwordRequired ? new TextEncoder().encode(params.passwordHash!.slice(0, 32)) : undefined
  const session = generateECDHKeyPair()
  const temporary = params.passwordRequired ? undefined : startLoginChannel(params.clientId)
  let channel: ReturnType<NonNullable<typeof temporary>['accept']> | undefined
  base.searchParams.set('cid', params.clientId)
  base.searchParams.set('auth', params.passwordRequired ? '1' : '2')

  return new Promise<LoginHandshakeResult>((resolve, reject) => {
    const ws = openSocket(base.toString())
    let settled = false
    const cleanup = () => {
      params.signal?.removeEventListener('abort', abort)
      window.clearTimeout(connectionTimeout)
      window.clearTimeout(deadline)
    }
    const fail = (reason: unknown) => {
      if (settled) return
      settled = true
      cleanup()
      ws.close(3001, typeof reason === 'string' ? reason : 'invalid_auth_response')
      reject(typeof reason === 'string' ? reason : 'invalid_auth_response')
    }
    const abort = () => fail('cancelled')
    params.signal?.addEventListener('abort', abort, { once: true })
    const connectionTimeout = window.setTimeout(() => fail('timeout'), 10000)
    const deadline = window.setTimeout(() => fail('login_expired'), 300000)
    const sendRequest = async () => {
      const ua = await getAccurateAgent()
      if (settled) return
      const request = JSON.stringify({
        browserName: ua.browser.name,
        browserVersion: ua.browser.version,
        osName: ua.os.name,
        osVersion: ua.os.version,
        isMobile: ua.isMobile,
        ecdhPublicKey: bitArrayToBase64(session.publicKey),
        ...(params.passwordRequired ? { password: params.passwordHash } : { loginContext: channel!.context }),
        ...(params.peer ? { peer: params.peer } : {}),
      })
      const encrypted = channel ? channel.encryptRequest(request) : chachaEncrypt(passwordKey!, request)
      ws.send(bitArrayToUint8Array(encrypted) as unknown as ArrayBuffer)
    }
    ws.onopen = () => {
      window.clearTimeout(connectionTimeout)
      if (temporary) ws.send(JSON.stringify(temporary.hello))
      else void sendRequest().catch(fail)
    }
    ws.onmessage = async (event: MessageEvent) => {
      if (settled) return
      try {
        if (temporary && !channel) {
          if (typeof event.data !== 'string' || event.data.length > 4096) throw 'invalid_auth_response'
          channel = temporary.accept(JSON.parse(event.data), signatureKey, params.expectedDeviceId)
          await sendRequest()
          return
        }
        if (typeof event.data === 'string') throw 'invalid_auth_response'
        const bytes = event.data instanceof ArrayBuffer ? new Uint8Array(event.data) : new Uint8Array(await event.data.arrayBuffer())
        if (settled) return
        const response = JSON.parse(channel ? channel.decryptResponse(bytes) : chachaDecrypt(passwordKey!, bytes))
        if (response.status === 'PENDING') {
          params.onPending?.(channel?.verificationCode)
          return
        }
        if (response.status !== 'COMPLETED' || !Number.isSafeInteger(response.timestamp) || Math.abs(getSyncedTimestamp() - response.timestamp) > 300000) throw 'signature_verification_failed'
        if (!response.clientId || (params.expectedDeviceId && response.clientId !== params.expectedDeviceId)) throw 'peer_identity_changed'
        if (channel ? response.loginContext !== channel.context : response.loginContext !== undefined) throw 'invalid_auth_response'
        const signed = `${response.clientId}|${response.status}|${response.ecdhPublicKey}|${response.timestamp}${response.chatPaired ? '|true' : ''}${channel ? `|${channel.context}` : ''}`
        if (!verifyEd25519Signature(signatureKey, signed, response.signature)) throw 'signature_verification_failed'
        const serverPublic = Uint8Array.from(atob(response.ecdhPublicKey), (character) => character.charCodeAt(0))
        const token = computeECDHSharedKey(session.secretKey, serverPublic)
        settled = true
        cleanup()
        ws.close()
        resolve({ clientId: response.clientId, token, signaturePublicKey: signatureKey, chatPaired: response.chatPaired === true })
      } catch (error) { fail(error) }
    }
    ws.onerror = () => fail('failed_connect_ws')
    ws.onclose = (event: CloseEvent) => fail(event.reason || 'failed')
  })
}
