import { isLocalMode } from '@/lib/device/local-mode'
import { ref, computed, onUnmounted } from 'vue'
import { readLocalAddresses, restartLocalServer } from '@/lib/api/local-server'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import toast from '@/components/toaster'
import { readHttpServer, restartHttpServer } from '@/lib/api/http-server'
import { applyValues } from '@/lib/prefs/core'
import { useOnlineClients } from './use-online-clients'
import { useI18n } from 'vue-i18n'
import emitter from '@/plugins/eventbus'
import { Channel, invoke } from '@/lib/api/native-invoke'
import { setLocalServerPort, setLocalServerHttpsPort } from '@/lib/api/api'
import { buildUrl } from '@/lib/url'
import type { VSelectOption } from '@/components/base/VSelect.vue'

const HTTP_PORTS = [8080, 8180, 8280, 8380, 8480, 8580, 8680, 8780, 8880, 8980, 80]
const HTTPS_PORTS = [8043, 8143, 8243, 8343, 8443, 8543, 8643, 8743, 8843, 8943, 443]

function portOptions(candidates: number[], current: number): VSelectOption[] {
  const list: VSelectOption[] = candidates.map((p) => ({ value: p, label: String(p) }))
  if (current && !candidates.includes(current)) {
    list.unshift({ value: current, label: `${current}` })
  }
  return list
}

export function useHttpServer() {
  const { t } = useI18n()
  const localMode = isLocalMode()
  const clients = localMode ? useOnlineClients() : { count: ref<number | null>(null), failed: ref(false) }
  const { app } = storeToRefs(useTempStore())
  const ips = ref<string[]>([])
  const loadingIps = ref(true)
  const addressError = ref(false)
  let disposed = false
  let timer: ReturnType<typeof setTimeout> | undefined
  let registration: number | undefined

  async function loadIps(showLoading = true) {
    if (disposed) return
    if (showLoading) loadingIps.value = true
    try {
      if (localMode) {
        ips.value = await readLocalAddresses()
      } else {
        const snapshot = await readHttpServer()
        if (disposed) return
        ips.value = snapshot.addresses
        app.value.httpPort = snapshot.state.httpPort
        app.value.httpsPort = snapshot.state.httpsPort
        clients.count.value = snapshot.onlineClients.length
        clients.failed.value = false
        applyValues({ remote_access: snapshot.remoteAccess, mdns_hostname: snapshot.mdnsHostname })
      }
      addressError.value = false
    } catch (e) {
      addressError.value = true
      if (!localMode) clients.failed.value = true
      console.error('network addresses failed', e)
    } finally {
      loadingIps.value = false
    }
  }

  async function refreshAddresses() {
    if (disposed) return
    if (!localMode) {
      await loadIps()
      return
    }
    loadingIps.value = true
    try {
      if (registration === undefined) {
        const channel = new Channel<Record<string, never>>()
        channel.onmessage = () => {
          void loadIps()
        }
        const id = await invoke<number>('native_register_network_changes', { channel })
        if (disposed) {
          await invoke('native_unregister_network_changes', { id })
          return
        }
        registration = id
      }
      await loadIps()
    } catch (error) {
      addressError.value = true
      console.error('Network changes registration failed', error)
    } finally {
      loadingIps.value = false
    }
  }
  async function pollServer() {
    await loadIps(loadingIps.value)
    if (!disposed) timer = setTimeout(pollServer, 2000)
  }
  if (localMode) void refreshAddresses()
  else void pollServer()
  onUnmounted(() => {
    disposed = true
    if (timer) clearTimeout(timer)
    if (registration !== undefined) void invoke('native_unregister_network_changes', { id: registration }).catch(console.error)
  })

  async function savePort(kind: 'http' | 'https', port: number): Promise<boolean> {
    try {
      const server = await (localMode ? restartLocalServer(kind, port) : restartHttpServer(kind, port))
      if (localMode) {
        setLocalServerPort(server.httpPort)
        setLocalServerHttpsPort(server.httpsPort)
      }
      app.value.httpPort = server.httpPort
      app.value.httpsPort = server.httpsPort
      if (server.error) throw new Error(server.error)
      if (!__IS_TAURI__) {
        const url = new URL(window.location.href)
        const activePort = url.protocol === 'https:' ? server.httpsPort : server.httpPort
        url.port = String(activePort)
        if (url.href !== window.location.href) {
          window.location.assign(url.href)
          return true
        }
      }
      emitter.emit('refetch_app')
      return true
    } catch (e) {
      console.error('save port failed', e)
      toast(t('failed'), 'error')
      return false
    }
  }

  const httpPort = computed(() => app.value?.httpPort ?? 0)
  const httpsPort = computed(() => app.value?.httpsPort ?? 0)
  const httpPortOptions = computed(() => portOptions(HTTP_PORTS, httpPort.value))
  const httpsPortOptions = computed(() => portOptions(HTTPS_PORTS, httpsPort.value))
  const httpAddresses = computed(() => ips.value.map((ip) => buildUrl('http', ip, httpPort.value)))
  const httpsAddresses = computed(() => ips.value.map((ip) => buildUrl('https', ip, httpsPort.value)))

  return {
    ips,
    count: clients.count,
    clientsFailed: clients.failed,
    httpPort,
    httpsPort,
    httpAddresses,
    httpsAddresses,
    loadingIps,
    addressError,
    httpPortOptions,
    httpsPortOptions,
    loadIps: refreshAddresses,
    savePort,
  }
}
