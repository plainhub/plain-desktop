import { invoke } from '@tauri-apps/api/core'

export interface NativeSystemSettings {
  mdnsHostname: string
  signaturePublicKey: string
}

export function readNativeSystemSettings(): Promise<NativeSystemSettings> {
  if (!__IS_TAURI__) return Promise.reject(new Error('Native settings unavailable'))
  return invoke<NativeSystemSettings>('native_system_settings')
}
