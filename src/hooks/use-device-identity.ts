import { gqlFetch } from '@/lib/api/gql-client'
import { isLocalMode } from '@/lib/device/local-mode'
import { loadDeviceIdentity, type DeviceIdentity } from '@/lib/device/identity'
import { ref } from 'vue'

export function useDeviceIdentity() {
  const identity = ref<DeviceIdentity | null>(null)
  const loading = ref(false)
  const saving = ref(false)

  async function load() {
    if (!isLocalMode()) return
    loading.value = true
    try {
      identity.value = await loadDeviceIdentity()
    } catch (e) {
      console.error('Device identity query failed', e)
    } finally {
      loading.value = false
    }
  }

  async function saveName(name: string) {
    if (!isLocalMode()) return
    saving.value = true
    try {
      const result = await gqlFetch('mutation($name: String!) { updateDeviceName(name: $name) }', { name })
      if (result.errors?.length) throw new Error(result.errors[0].message)
      if (identity.value) {
        identity.value = { ...identity.value, deviceName: name }
      }
    } catch (e) {
      console.error('Device name update failed', e)
    } finally {
      saving.value = false
    }
  }

  return { identity, loading, saving, load, saveName }
}
