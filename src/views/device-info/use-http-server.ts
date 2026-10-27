import { ref, computed, onUnmounted } from 'vue'
import { readLocalAddresses, restartLocalServer } from '@/lib/api/local-server'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import toast from '@/components/toaster'
import { useI18n } from 'vue-i18n'
import emitter from '@/plugins/eventbus'
import { Channel, invoke } from '@tauri-apps/api/core'
import { setLocalServerPort, setLocalServerHttpsPort } from '@/lib/api/api'
import { buildUrl } from '@/lib/url'
import type { VSelectOption } from '@/components/base/VSelect.vue'

// Mirrors plain-app's `web/HttpServerPorts.kt` — fixed port candidates
// the user can pick from in the edit dialog. The server tries each in
// order and falls back to an OS-assigned port only when every candidate
// is taken.
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
  const { app } = storeToRefs(useTempStore())
  const ips = ref<string[]>([])
  const saving = ref(false)

  async function loadIps() {
    if (!__IS_TAURI__) return
    try {
      ips.value = await readLocalAddresses()
    } catch (e) {
      console.error('network addresses failed', e)
    }
  }

  let disposed = false
  let registration: number | undefined
  if (__IS_TAURI__) {
    const channel = new Channel<Record<string, never>>()
    channel.onmessage = () => { void loadIps() }
    void invoke<number>('native_register_network_changes', { channel }).then((id) => {
      if (disposed) return invoke('native_unregister_network_changes', { id })
      registration = id
      return loadIps()
    }).catch((error) => console.error('Network changes registration failed', error))
  }
  onUnmounted(() => {
    disposed = true
    if (registration !== undefined) void invoke('native_unregister_network_changes', { id: registration }).catch(console.error)
  })

  async function savePort(kind: 'http' | 'https', port: number): Promise<boolean> {
    if (!__IS_TAURI__) return false
    saving.value = true
    try {
      const server = await restartLocalServer(kind, port)
      setLocalServerPort(server.httpPort)
      setLocalServerHttpsPort(server.httpsPort)
      if (server.error) throw new Error(server.error)
      emitter.emit('refetch_app')
      toast(t('saved'))
      return true
    } catch (e) {
      console.error('save port failed', e)
      toast(t('failed'), 'error')
      return false
    } finally {
      saving.value = false
    }
  }

  const httpPort = computed(() => app.value?.httpPort ?? 0)
  const httpsPort = computed(() => app.value?.httpsPort ?? 0)
  const httpPortOptions = computed(() => portOptions(HTTP_PORTS, httpPort.value))
  const httpsPortOptions = computed(() => portOptions(HTTPS_PORTS, httpsPort.value))
  const httpAddresses = computed(() =>
    ips.value.map((ip) => buildUrl('http', ip, httpPort.value))
  )
  const httpsAddresses = computed(() =>
    ips.value.map((ip) => buildUrl('https', ip, httpsPort.value))
  )

  return {
    ips, httpPort, httpsPort, httpAddresses, httpsAddresses,
    httpPortOptions, httpsPortOptions, loadIps, savePort,
  }
}
