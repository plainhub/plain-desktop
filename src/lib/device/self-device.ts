import { GqlError } from '@/lib/api/gql-error'
import { readNativeDeviceIdentity } from './native-settings'
import { readLocalAddresses } from '@/lib/api/local-server'
import { gqlFetch, type bindGraphqlClient } from '@/lib/api/gql-client'
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

export async function loadSelfDevice(request?: ReturnType<typeof bindGraphqlClient>): Promise<SelfDevice | null> {
  if (!request && !isLocalMode()) return null
  try {
    const result = await (request ?? gqlFetch)<{ app: { clientId: string; deviceName: string; httpsPort: number; deviceType: DeviceType } }>(
      'query { app { clientId deviceName httpsPort deviceType } }',
    )
    if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
    const app = result.data.app
    const ips = __IS_TAURI__ ? await readLocalAddresses() : []
    const port = app.httpsPort
    const ip = ips[0] || ''
    return {
      id: app.clientId,
      name: app.deviceName || '',
      host: ip ? `${ip}:${port}` : '',
      deviceType: app.deviceType,
      ips,
      port,
      publicKey: __IS_TAURI__ ? (await readNativeDeviceIdentity()).signaturePublicKey : '',
    }
  } catch (e) {
    console.error('load self device failed', e)
    return null
  }
}
