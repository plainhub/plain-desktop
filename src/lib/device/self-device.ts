import { readNativeDeviceIdentity } from './native-settings'
import { readLocalAddresses } from '@/lib/api/local-server'
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
    const result = await gqlFetch<{ app: { clientId: string; deviceName: string; httpsPort: number; deviceType: DeviceType } }>(
      'query { app { clientId deviceName httpsPort deviceType } }',
    )
    if (result.errors?.length) throw new Error(result.errors[0].message)
    const app = result.data.app
    const ips = await readLocalAddresses()
    const port = app.httpsPort
    const ip = ips[0] || ''
    return {
      id: app.clientId,
      name: app.deviceName || '',
      host: ip ? `${ip}:${port}` : '',
      deviceType: app.deviceType,
      ips,
      port,
      publicKey: (await readNativeDeviceIdentity()).signaturePublicKey,
    }
  } catch (e) {
    console.error('load self device failed', e)
    return null
  }
}
