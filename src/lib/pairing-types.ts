export interface PairingRequest {
  verificationCode?: string
  fromId: string
  fromName: string
  port: number
  deviceType: string
  ecdhPublicKey: string
  signaturePublicKey: string
  timestamp: number
  ips: string[]
  signature: string
  /** Plain-app-only field: the requester's IP. Missing on plain-web's local
   * Rust server, so we keep it optional for cross-server compatibility. */
  fromIp?: string
}

export interface PairingResult {
  deviceId: string
  deviceName: string
  /** Empty string for `success` / `cancelled`; human-readable reason for `failed`. */
  error: string
}
