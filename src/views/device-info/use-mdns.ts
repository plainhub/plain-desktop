import { ref } from 'vue'
import { gqlFetch } from '@/lib/api/gql-client'
import { isLocalMode } from '@/lib/device/local-mode'
import toast from '@/components/toaster'
import { useI18n } from 'vue-i18n'

// Wire shape of the Rust `MdnsServiceSnapshot` — mirrors plain-app's
// `MdnsServiceSnapshot` shown on the mDNS debug page.
export interface MdnsServiceSnapshot {
  serviceType: string
  instanceName: string
  instanceFqdn: string
  hostname: string
  port: number
  txtRecords: string[]
  ips: string[]
  ipv6: string[]
  complete: boolean
}

export interface MdnsActivity {
  time: number
  event: string
  detail: string
}

// Mirrors plain-app's MdnsDebugPage: while the page is visible it keeps
// periodic mDNS discovery running and refreshes the browser snapshot every
// two seconds; discovery is stopped on exit only when this page started it.
export function useMdns() {
  const { t } = useI18n()
  const hostname = ref('')
  const snapshots = ref<MdnsServiceSnapshot[]>([])
  const activity = ref<MdnsActivity[]>([])
  const paused = ref(false)
  const saving = ref(false)
  const hostnameInvalid = ref(false)
  let sessionToken: string | undefined
  let generation = 0
  let browsing = false
  let timer: ReturnType<typeof setInterval> | undefined

  async function loadHostname() {
    if (!isLocalMode()) return
    const result = await gqlFetch<{ systemSettings: { mdnsHostname: string } }>('query { systemSettings { mdnsHostname } }')
    if (result.errors?.length) throw new Error(result.errors[0].message)
    hostname.value = result.data.systemSettings.mdnsHostname
  }

  // Mirrors plain-app's MdnsAndPortEditDialog validation: non-blank and
  // ending with ".local".
  async function saveHostname(value: string): Promise<boolean> {
    const h = value.trim()
    hostnameInvalid.value = h.length === 0 || !h.endsWith('.local')
    if (hostnameInvalid.value) return false
    saving.value = true
    try {
      const result = await gqlFetch('mutation($input: SystemSettingsInput!) { updateSystemSettings(input: $input) { result { affectedCount } } }', { input: { action: 'SET_MDNS_HOSTNAME', hostname: h } })
      if (result.errors?.length) throw new Error(result.errors[0].message)
      hostname.value = h
      toast(t('saved'))
      await refreshSnapshot()
      return true
    } catch (e) {
      console.error('set mdns hostname failed', e)
      hostnameInvalid.value = true
      return false
    } finally {
      saving.value = false
    }
  }

  async function refreshSnapshot() {
    try {
      const result = await gqlFetch<{ mdnsDebug: { services: MdnsServiceSnapshot[]; activity: MdnsActivity[] } }>(
        'query { mdnsDebug { services { serviceType instanceName instanceFqdn hostname port txtRecords ips ipv6 complete } activity { time event detail } } }',
        undefined, { fresh: true },
      )
      if (result.errors?.length) throw new Error(result.errors[0].message)
      snapshots.value = result.data.mdnsDebug.services
      activity.value = result.data.mdnsDebug.activity
    } catch (e) {
      console.error('mDNS debug query failed', e)
    }
  }

  async function release(token: string) {
    const result = await gqlFetch('mutation($token: String!) { releaseMdnsDebugSession(token: $token) { debugSessionCount } }', { token })
    if (result.errors?.length) throw new Error(result.errors[0].message)
  }

  async function startBrowsing() {
    if (!isLocalMode() || browsing) return
    browsing = true
    const current = ++generation
    const token = crypto.randomUUID()
    try {
      const result = await gqlFetch('mutation($token: String!) { acquireMdnsDebugSession(token: $token) { debugSessionCount } }', { token })
      if (result.errors?.length) throw new Error(result.errors[0].message)
      if (!browsing || generation !== current) {
        await release(token)
        return
      }
      sessionToken = token
      await refreshSnapshot()
      if (browsing && generation === current) {
        timer = setInterval(() => {
          if (!paused.value) refreshSnapshot()
        }, 2000)
      }
    } catch (e) {
      if (generation === current) browsing = false
      console.error('mDNS debug session failed', e)
    }
  }

  function stopBrowsing() {
    browsing = false
    generation++
    if (timer) {
      clearInterval(timer)
      timer = undefined
    }
    const token = sessionToken
    sessionToken = undefined
    if (token) release(token).catch((error) => console.error('mDNS debug session release failed', error))
  }

  return {
    hostname, snapshots, activity, paused,
    loadHostname, saveHostname, refreshSnapshot, startBrowsing, stopBrowsing,
  }
}
