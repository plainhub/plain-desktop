import { invoke } from '@tauri-apps/api/core'
import { isLocalMode } from '@/lib/device/local-mode'
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

export async function loadQrPairingCode(): Promise<QrPairingCode | null> {
  if (!isLocalMode()) return null
  try {
    const payload = await invoke<string>('native_pairing_code')
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
    deviceType: request.deviceType || 'OTHER',
  }
}
