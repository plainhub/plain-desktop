import { invoke } from '@tauri-apps/api/core'

export interface NativeDeviceIdentity {
  clientId: string
  signaturePublicKey: string
}

export function readNativeDeviceIdentity(): Promise<NativeDeviceIdentity> {
  if (!__IS_TAURI__) return Promise.reject(new Error('Native identity unavailable'))
  return invoke<NativeDeviceIdentity>('native_device_identity')
}
