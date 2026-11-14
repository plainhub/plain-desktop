import type { DeviceType } from '@/lib/status'

export interface PairingRequest {
  verificationCode: string
  identityChanged: boolean
  fromId: string
  fromName: string
  fromIp: string
  deviceType: DeviceType
}

export interface PairingResult {
  deviceId: string
  deviceName: string
  ip?: string
  port?: number | null
  deviceType?: DeviceType | null
  requestId?: string | null
  /** Empty string for `success` / `canceled`; human-readable reason for `failed`. */
  error: string
}
