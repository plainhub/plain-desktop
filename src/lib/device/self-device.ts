import { invoke } from '@tauri-apps/api/core'
import { DeviceType } from '@/lib/status'

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
  if (!__IS_TAURI__) return null
  try {
    const [identity, ips, port] = await Promise.all([
      invoke<{ clientId: string; deviceName: string; publicKey: string }>('get_device_identity'),
      invoke<string[]>('local_ipv4_strs'),
      invoke<number>('local_server_https_port'),
    ])
    const ip = ips.find((v) => !v.startsWith('127.')) || ips[0] || ''
    return {
      id: identity.clientId,
      name: identity.deviceName || '',
      host: ip ? `${ip}:${port}` : '',
      deviceType: DeviceType.COMPUTER,
      ips,
      port,
      publicKey: identity.publicKey,
    }
  } catch (e) {
    console.error('load self device failed', e)
    return null
  }
}
