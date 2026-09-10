import { ref } from 'vue'
import { initMutation, startDiscoveryGQL, stopDiscoveryGQL } from '@/lib/api/mutation'
import { gqlFetch } from '@/lib/api/gql-client'
import { isDiscoveringGQL } from '@/lib/api/query'
import emitter from '@/plugins/eventbus'
import type { PairingResult } from '@/lib/pairing-types'
import { useChatStore } from '@/stores/chat'
import { PeerStatus } from '@/lib/status'

export type DiscoveryDeviceStatus = 'PAIRING' | 'UNPAIRING' | PeerStatus

export interface DiscoveredDevice {
  id: string
  name: string
  ips: string[]
  port: number
  deviceType: string
  version: string
  platform: string
  lastSeen: string
  status: DiscoveryDeviceStatus
  discoveryMethods: string[]
}

export enum DiscoveryStatus {
  IDLE,
  SEARCHING,
  OK,
  PERMISSION_DENIED,
  NETWORK_ERROR,
}

const devices = ref<DiscoveredDevice[]>([])
const status = ref<DiscoveryStatus>(DiscoveryStatus.IDLE)

let activeCount = 0
let listenerInitialized = false
let pollTimer: ReturnType<typeof setInterval> | null = null

// The server-side scan can be torn down behind our back — e.g. the app closes
// its own nearby page and stops discovery for every connected client. Poll
// while a page is watching and re-issue startDiscovery when it went down.
async function checkAndEnsureDiscovering() {
  if (activeCount === 0) return
  try {
    const r = await gqlFetch<{ isDiscovering: boolean }>(isDiscoveringGQL)
    if (!r.errors && r.data?.isDiscovering !== true) {
      await startMutate()
    }
  } catch {
    // ignore polling errors
  }
}

function startPolling() {
  if (pollTimer !== null) return
  pollTimer = setInterval(checkAndEnsureDiscovering, 5_000)
}

function stopPolling() {
  if (pollTimer !== null) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

function ensureListener() {
  if (listenerInitialized) return
  listenerInitialized = true

  emitter.on('nearby_device_found', (device: DiscoveredDevice) => {
    upsertDiscoveredDevice(device)
  })

  emitter.on('nearby_discovery_started', () => {
    status.value = DiscoveryStatus.SEARCHING
  })

  emitter.on('nearby_discovery_stopped', (payload: any) => {
    status.value = payload?.reason === 'no_receivers' ? DiscoveryStatus.NETWORK_ERROR : DiscoveryStatus.IDLE
  })

  emitter.on('pairing_success', (result: PairingResult) => {
    const device = devices.value.find((d) => d.id === result.deviceId)
    if (device) {
      device.status = PeerStatus.PAIRED
    }
    useChatStore().fetchPeers()
  })

  emitter.on('pairing_failed', (result: PairingResult) => {
    const device = devices.value.find((d) => d.id === result.deviceId)
    if (device) {
      device.status = PeerStatus.UNPAIRED
    }
  })
  
  emitter.on('pairing_canceled', (result: PairingResult) => {
    const device = devices.value.find((d) => d.id === result.deviceId)
    if (device) {
      device.status = PeerStatus.UNPAIRED
    }
  })

  emitter.on('pairing_started', (result: PairingResult) => {
    const device = devices.value.find((d) => d.id === result.deviceId)
    if (device) {
      device.status = 'PAIRING'
    }
  })
}

const { mutate: startMutate } = initMutation({ document: startDiscoveryGQL }, false)
const { mutate: stopMutate } = initMutation({ document: stopDiscoveryGQL }, false)

/** Insert or refresh a device in the discovery list. Also used by the
 * QR-pairing fallback, whose paired devices never passed through mDNS. */
export function upsertDiscoveredDevice(device: DiscoveredDevice) {
  const existing = devices.value.findIndex((d) => d.id === device.id)
  if (existing >= 0) {
    const next = devices.value.slice()
    next[existing] = { ...next[existing], ...device }
    devices.value = next
  } else {
    devices.value = [...devices.value, device]
  }
  status.value = DiscoveryStatus.OK
}

export function useDeviceDiscovery() {
  ensureListener()

  async function start() {
    activeCount += 1
    status.value = DiscoveryStatus.SEARCHING
    await startMutate()
    startPolling()
  }

  async function stop() {
    if (activeCount === 0) return
    activeCount -= 1
    if (activeCount > 0) return
    stopPolling()
    await stopMutate()
    status.value = DiscoveryStatus.IDLE
    devices.value = []
  }

  function retry() {
    status.value = DiscoveryStatus.SEARCHING
    void startMutate()
  }

  async function openLanPermissionSettings() {
    if (__IS_TAURI__) {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl('x-apple.systempreferences:').catch(async () => {
        await openUrl('https://support.apple.com/guide/mac-help/mchla4f49138/mac')
      })
      return
    }
    if (typeof window === 'undefined') return
    window.location.href = 'x-apple.systempreferences:'
  }

  return { devices, status, start, stop, retry, openLanPermissionSettings }
}