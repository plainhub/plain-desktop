import { isLocalMode } from './local-mode'
import { readNativeDeviceIdentity } from './native-settings'
import { requestInit } from '@/lib/api/init'
import { gqlFetch } from '@/lib/api/gql-client'

export interface DeviceIdentity {
  clientId: string
  deviceName: string
  publicKey: string
}

export async function loadDeviceIdentity(): Promise<DeviceIdentity> {
  const result = await gqlFetch<{ app: { clientId: string; deviceName: string } }>(
    'query { app { clientId deviceName } }',
  )
  if (result.errors?.length) throw new Error(result.errors[0].message)
  return { ...result.data.app, publicKey: (isLocalMode() ? (await readNativeDeviceIdentity()).signaturePublicKey : (await requestInit()).data?.signaturePublicKey ?? '') }
}
