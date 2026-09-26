import { chachaEncrypt, chachaDecrypt, bitArrayToUint8Array, bitArrayToBase64, generateECDHKeyPair, computeECDHSharedKey, verifyEd25519Signature } from '@/lib/api/crypto'
import { getWebSocketBaseUrl } from '@/lib/api/api'
import { getAccurateAgent } from '@/lib/agent/agent'
import { openSocket } from '@/lib/api/http'
import { getSyncedTimestamp } from '@/lib/api/time-sync'
import type { DeviceType } from '@/lib/status'

/** Window (ms) within which a login response timestamp is considered fresh. */
const SIGNATURE_FRESHNESS_MS = 5 * 60 * 1000

export interface LoginHandshakeParams {
  /** SHA-512 hash of the password. */
  passwordHash: string
  clientId: string
  /** Server Ed25519 public key previously trusted for this clientId, if any. */
  storedSignaturePublicKey?: string
  /** Server Ed25519 public key returned by `/init` for this session. */
  initSignaturePublicKey?: string
  /** Called when the server asks for 2FA confirmation (PENDING). */
  onPending?: () => void
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
  /** The server's device clientId (from AuthResponse), used as the session key. */
  clientId: string
  token: string
  /** The server public key that actually passed verification (trusted). */
  signaturePublicKey: string
  chatPaired: boolean
}

function buildSignatureData(r: { clientId: string; status: string; ecdhPublicKey: string; timestamp: number; chatPaired?: boolean }): string {
  return `${r.clientId}|${r.status}|${r.ecdhPublicKey}|${r.timestamp}${r.chatPaired ? '|true' : ''}`
}

/**
 * TOFU verification: prefer the stored key; if it fails (app reinstall / key
 * rotation), fall back to the `/init` key. Only reject when both fail.
 */
function verifyLoginSignature(
  r: { clientId: string; status: string; ecdhPublicKey: string; timestamp: number; signature: string; chatPaired?: boolean },
  storedKey: string | undefined,
  initKey: string | undefined,
): { verified: boolean; usedKey?: string } {
  const data = buildSignatureData(r)
  const tryVerify = (k?: string): boolean => !!k && verifyEd25519Signature(k, data, r.signature)

  if (tryVerify(storedKey)) return { verified: true, usedKey: storedKey }
  if (tryVerify(initKey)) return { verified: true, usedKey: initKey }
  return { verified: false }
}

function isFresh(timestamp: number): boolean {
  return Math.abs(getSyncedTimestamp() - timestamp) <= SIGNATURE_FRESHNESS_MS
}

/**
 * Perform the WebSocket login handshake: generate a client ECDH key pair,
 * send the encrypted `AuthRequest`, verify the signed response, and derive
 * the session token locally (never transmitted).
 *
 * Resolves with the token on success; rejects with an error message key on
 * any failure (verification, timeout, or connection drop).
 */
export function performLoginHandshake(params: LoginHandshakeParams): Promise<LoginHandshakeResult> {
  const { passwordHash, clientId, onPending } = params
  if (params.signal?.aborted) return Promise.reject('cancelled')
  const key = new Uint8Array(passwordHash.slice(0, 32).split('').map((c) => c.charCodeAt(0)))

  // Generate client ECDH key pair for token exchange
  const clientKeyPair = generateECDHKeyPair()
  const clientPubBase64 = bitArrayToBase64(clientKeyPair.publicKey)

  return new Promise<LoginHandshakeResult>((resolve, reject) => {
    const wsUrl = `${getWebSocketBaseUrl()}?cid=${clientId}&auth=1`
    const ws = openSocket(wsUrl)
    let timeoutId: number | undefined
    const abort = () => {
      cleanup()
      ws.close(3001, 'cancelled')
      reject('cancelled')
    }
    params.signal?.addEventListener('abort', abort, { once: true })

    const cleanup = () => {
      params.signal?.removeEventListener('abort', abort)
      window.clearTimeout(timeoutId)
    }

    ws.onopen = async () => {
      const ua = await getAccurateAgent()
      if (params.signal?.aborted) return
      const browserName = __IS_TAURI__ ? 'PlainApp' : ua.browser.name
      const browserVersion = __IS_TAURI__ ? '' : ua.browser.version
      const enc = chachaEncrypt(key, JSON.stringify({
        password: passwordHash,
        browserName,
        browserVersion,
        osName: ua.os.name,
        osVersion: ua.os.version,
        isMobile: ua.isMobile,
        ecdhPublicKey: clientPubBase64,
        ...(params.peer ? { peer: params.peer } : {}),
      }))
      ws.send(bitArrayToUint8Array(enc) as unknown as ArrayBuffer)
    }

    ws.onmessage = async (event: MessageEvent) => {
      if (params.signal?.aborted) return
      const d = chachaDecrypt(key, new Uint8Array(await event.data.arrayBuffer()))
      const r = JSON.parse(d)
      if (r.status === 'PENDING') {
        onPending?.()
        return
      }

      // Reject stale responses (replay protection)
      if (!isFresh(r.timestamp)) {
        ws.close(3001, 'signature_verification_failed')
        reject('signature_verification_failed')
        return
      }

      const { verified, usedKey } = verifyLoginSignature(r, params.storedSignaturePublicKey, params.initSignaturePublicKey)
      if (!verified || !usedKey) {
        ws.close(3001, 'signature_verification_failed')
        reject('signature_verification_failed')
        return
      }

      // Compute ECDH shared key → token (never transmitted)
      const serverPubBytes = Uint8Array.from(atob(r.ecdhPublicKey), (c) => c.charCodeAt(0))
      const token = computeECDHSharedKey(clientKeyPair.secretKey, serverPubBytes)
      ws.close()
      cleanup()
      resolve({ clientId: r.clientId, token, signaturePublicKey: usedKey, chatPaired: r.chatPaired === true })
    }

    ws.onclose = (event: CloseEvent) => {
      cleanup()
      if (event.reason === 'OK') return
      reject(event.reason || 'failed')
    }

    timeoutId = window.setTimeout(() => { if (ws.readyState !== 1) ws.close(3001, 'timeout') }, 5000)
  })
}
