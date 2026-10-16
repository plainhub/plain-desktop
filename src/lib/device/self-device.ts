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
    const result = await gqlFetch<{ discoveryReply: { id: string; name: string; ips: string[]; port: number; deviceType: DeviceType }; systemSettings: { signaturePublicKey: string } }>(
      'query { discoveryReply { id name ips port deviceType } systemSettings { signaturePublicKey } }',
    )
    if (result.errors?.length) throw new Error(result.errors[0].message)
    const reply = result.data.discoveryReply
    const { ips, port } = reply
    const ip = ips.find((v) => !v.startsWith('127.')) || ips[0] || ''
    return {
      id: reply.id,
      name: reply.name || '',
      host: ip ? `${ip}:${port}` : '',
      deviceType: reply.deviceType,
      ips,
      port,
      publicKey: result.data.systemSettings.signaturePublicKey,
    }
  } catch (e) {
    console.error('load self device failed', e)
    return null
  }
}
