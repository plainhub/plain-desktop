import { DeviceType } from '@/lib/status'
import { invoke } from '@/lib/api/native-invoke'
import { isLocalMode } from '@/lib/device/local-mode'
import { qrCodeSvg } from '@/lib/qr'

export interface QrPairingCode {
  requestId: string
  verificationCode: string
  svg: string
}

export interface QrPairedDevice {
  id: string
  name: string
  ip: string
  port: number
  host: string
  deviceType: DeviceType
}

export async function loadQrPairingCode(): Promise<QrPairingCode | null> {
  if (!isLocalMode()) return null
  try {
    const { payload, requestId, verificationCode } = await invoke<{ payload: string; requestId: string; verificationCode: string }>('native_pairing_code')
    return { requestId, verificationCode, svg: qrCodeSvg(payload, { border: 2 }) }
  } catch (e) {
    console.error('load qr pairing code failed', e)
    return null
  }
}
