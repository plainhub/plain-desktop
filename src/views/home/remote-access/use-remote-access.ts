import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import * as prefs from '@/lib/prefs/core'
import toast from '@/components/toaster'

export function useRemoteAccess() {
  const { t } = useI18n()
  const saving = ref(false)
  async function save(value: boolean) {
    saving.value = true
    try {
      await prefs.set('remote_access', value)
    } catch (error) {
      toast(t(error instanceof Error ? error.message : 'network_error'), 'error')
    } finally {
      saving.value = false
    }
  }
  return {
    enabled: computed(() => prefs.get('remote_access', true)),
    hostname: computed(() => prefs.get('mdns_hostname', '')),
    saving,
    save,
  }
}
