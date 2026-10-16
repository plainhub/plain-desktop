import { gqlFetch } from '@/lib/api/gql-client'
import { isLocalMode } from '@/lib/device/local-mode'
import type { DeviceType } from '@/lib/status'

export interface SelfDevice {
  id: string
  name: string
  host: string
  deviceType: DeviceType
  ips: string[]
  port: number
  publicKey: string
}

export async function loadSelfDevice(): Promise<SelfDevice | null> {
  if (!isLocalMode()) return null
  try {
    const result = await gqlFetch<{ app: { clientId: string; deviceName: string; httpsPort: number; deviceType: DeviceType; network: { ipv4Addresses: string[]; primaryIpv4: string } }; systemSettings: { signaturePublicKey: string } }>(
      'query { app { clientId deviceName httpsPort deviceType network { ipv4Addresses primaryIpv4 } } systemSettings { signaturePublicKey } }',
    )
    if (result.errors?.length) throw new Error(result.errors[0].message)
    const app = result.data.app
    const ips = app.network.ipv4Addresses
    const port = app.httpsPort
    const ip = app.network.primaryIpv4
    return {
      id: app.clientId,
      name: app.deviceName || '',
      host: ip ? `${ip}:${port}` : '',
      deviceType: app.deviceType,
      ips,
      port,
      publicKey: result.data.systemSettings.signaturePublicKey,
    }
  } catch (e) {
    console.error('load self device failed', e)
    return null
  }
}
