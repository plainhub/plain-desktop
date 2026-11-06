import { isLocalMode } from '@/lib/device/local-mode'
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
  const loadingIps = ref(true)
  const addressError = ref(false)
  let disposed = false
  let registration: number | undefined

  async function loadIps() {
    if (!isLocalMode() || disposed) return
    loadingIps.value = true
    try {
      ips.value = await readLocalAddresses()
      addressError.value = false
    } catch (e) {
      addressError.value = true
      console.error('network addresses failed', e)
    } finally {
      loadingIps.value = false
    }
  }

  async function refreshAddresses() {
    if (!isLocalMode() || disposed) return
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
  void refreshAddresses()
  onUnmounted(() => {
    disposed = true
    if (registration !== undefined) void invoke('native_unregister_network_changes', { id: registration }).catch(console.error)
  })

  async function savePort(kind: 'http' | 'https', port: number): Promise<boolean> {
    if (!isLocalMode()) return false
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
