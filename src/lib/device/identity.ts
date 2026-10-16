import { gqlFetch } from '@/lib/api/gql-client'

export interface DeviceIdentity {
  clientId: string
  deviceName: string
  publicKey: string
}

export async function loadDeviceIdentity(): Promise<DeviceIdentity> {
  const result = await gqlFetch<{ app: { clientId: string; deviceName: string }; systemSettings: { signaturePublicKey: string } }>(
    'query { app { clientId deviceName } systemSettings { signaturePublicKey } }',
  )
  if (result.errors?.length) throw new Error(result.errors[0].message)
  return { ...result.data.app, publicKey: result.data.systemSettings.signaturePublicKey }
}
