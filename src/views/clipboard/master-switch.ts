import { ref, watch } from 'vue'
import { bindGraphqlClient } from '@/lib/api/gql-client'
import { setPrefGQL } from '@/lib/api/mutation'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { isLocalMode } from '@/lib/device/local-mode'
import { preferencesQuery } from '@/lib/prefs/core'

const KEY = 'api_permissions'
const CLIPBOARD = 'CLIPBOARD'

export function useClipboardMasterSwitch(visible: () => boolean) {
  const enabled = ref<boolean>()
  const saving = ref(false)
  let refresh: (() => Promise<void>) | undefined

  watch(visible, (shown, _, cleanup) => {
    enabled.value = undefined
    if (!shown || !isLocalMode()) return
    const request = bindGraphqlClient()
    let active = true
    let sequence = 0
    const load = async () => {
      const reading = ++sequence
      try {
        const result = await request<{ prefs: Record<string, unknown> }>(preferencesQuery, { keys: [KEY] })
        if (!active || reading !== sequence) return
        const permissions = result.data?.prefs?.[KEY]
        enabled.value = !result.errors?.length && Array.isArray(permissions) ? permissions.includes(CLIPBOARD) : undefined
      } catch {
        if (active && reading === sequence) enabled.value = undefined
      }
    }
    refresh = load
    const stop = graphqlSubscriptions.subscribe('prefsChanged', load)
    cleanup(() => { active = false; refresh = undefined; stop() })
    void load()
  }, { immediate: true })

  async function setEnabled(checked: boolean) {
    if (!visible() || !isLocalMode() || saving.value || enabled.value === undefined || enabled.value === checked) return
    saving.value = true
    try {
      const result = await bindGraphqlClient()<{ setPref: boolean }>(setPrefGQL, { key: KEY, value: { [CLIPBOARD]: checked } })
      if (result.errors?.length || !result.data?.setPref) throw new Error('Clipboard permission write refused')
      if (refresh) await refresh()
    } finally {
      saving.value = false
    }
  }

  return { enabled, saving, setEnabled }
}
