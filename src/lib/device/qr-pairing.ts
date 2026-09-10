import { invoke } from '@tauri-apps/api/core'
import { qrCodeSvg } from '@/lib/qr'
import type { PairingRequest } from '@/lib/pairing-types'

export interface QrPairingCode {
  payload: string
  svg: string
}

export interface QrPairedDevice {
  id: string
  name: string
  ip: string
  port: number
  host: string
  deviceType: string
}

export function buildQrPairPayload(
  identity: { clientId: string; deviceName: string },
  ips: string[],
  port: number,
): string {
  return `plainapp://pair?v=1&id=${identity.clientId}&name=${encodeURIComponent(identity.deviceName)}&ips=${ips.join(',')}&port=${port}`
}

export async function loadQrPairingCode(): Promise<QrPairingCode | null> {
  if (!__IS_TAURI__) return null
  try {
    const [identity, ips, port] = await Promise.all([
      invoke<{ clientId: string; deviceName: string }>('get_device_identity'),
      invoke<string[]>('local_ipv4_strs'),
      invoke<number>('local_server_https_port'),
    ])
    const payload = buildQrPairPayload(identity, ips.filter((v) => v && !v.startsWith('127.')), port)
    return { payload, svg: qrCodeSvg(payload, { border: 2 }) }
  } catch (e) {
    console.error('load qr pairing code failed', e)
    return null
  }
}

/** The phone announced itself over `POST /nearby` — this is where it is reachable. */
export function qrPairedDevice(request: PairingRequest): QrPairedDevice {
  const ip = request.fromIp || request.ips[0] || ''
  return {
    id: request.fromId,
    name: request.fromName,
    ip,
    port: request.port,
    host: ip ? `${ip}:${request.port}` : '',
    deviceType: request.deviceType || 'PHONE',
  }
}
