import { ref } from 'vue'
import { acquireDiscoveryGQL, releaseDiscoveryGQL } from '@/lib/api/mutation'
import { bindGraphqlClient } from '@/lib/api/gql-client'
import { nearbyDevicesGQL } from '@/lib/api/query'
import { randomUUID } from '@/lib/strutil'
import emitter from '@/plugins/eventbus'
import type { PairingResult } from '@/lib/pairing-types'
import { useChatStore } from '@/stores/chat'
import type { PairingState } from '@/lib/device/pairing'
import { PeerStatus } from '@/lib/status'

export type DiscoveryDeviceStatus = 'PAIRING' | 'UNPAIRING' | PeerStatus

export interface DiscoveredDevice {
  id: string
  name: string
  ips: string[]
  bestIp?: string
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

const activeSessions = new Set<() => Promise<void>>()
let listenerInitialized = false

const pairingStates = new Map<string, PairingState>()
const terminalPairing = new Set(['SUCCEEDED', 'FAILED', 'CANCELLED'])

function applyPairingState(state: PairingState) {
  if (!state.deviceId) return
  const device = devices.value.find((d) => d.id === state.deviceId)
  if (!device) return
  pairingStates.set(state.deviceId, state)
  device.status = state.phase === 'SUCCEEDED' ? PeerStatus.PAIRED : terminalPairing.has(state.phase) ? useChatStore().findPeer(state.deviceId)?.status ?? PeerStatus.UNPAIRED : 'PAIRING'
}
function restoreDeviceStatus(deviceId: string) {
  const state = pairingStates.get(deviceId)
  if (state && !terminalPairing.has(state.phase)) return
  const device = devices.value.find((d) => d.id === deviceId)
  if (device) device.status = useChatStore().findPeer(deviceId)?.status ?? PeerStatus.UNPAIRED
}

function ensureListener() {
  if (listenerInitialized) return
  listenerInitialized = true

  emitter.on('NEARBY_DEVICE_FOUND', (device: DiscoveredDevice) => {
    upsertDiscoveredDevice(device)
  })

  emitter.on('NEARBY_DEVICE_UNREACHABLE', ({ id }) => {
    devices.value = devices.value.filter((device) => device.id !== id)
  })

  emitter.on('app_socket_connection_changed', (connected) => {
    if (connected) for (const acquire of activeSessions) void acquire()
  })

  emitter.on('NEARBY_DISCOVERY_STARTED', () => {
    status.value = DiscoveryStatus.SEARCHING
  })

  emitter.on('NEARBY_DISCOVERY_STOPPED', (payload: any) => {
    status.value = payload?.reason === 'no_receivers' ? DiscoveryStatus.NETWORK_ERROR : DiscoveryStatus.IDLE
  })

  emitter.on('PAIRING_SUCCESS', (result: PairingResult) => {
    const device = devices.value.find((d) => d.id === result.deviceId)
    if (device) {
      device.status = PeerStatus.PAIRED
    }
    useChatStore().fetchPeers()
  })

  emitter.on('PAIRING_STATE_CHANGED', applyPairingState)
  emitter.on('PAIRING_FAILED', (result: PairingResult) => restoreDeviceStatus(result.deviceId))
  
  emitter.on('PAIRING_CANCELED', (result: PairingResult) => restoreDeviceStatus(result.deviceId))

  emitter.on('PAIRING_STARTED', (result: PairingResult) => {
    const device = devices.value.find((d) => d.id === result.deviceId)
    if (device) {
      device.status = 'PAIRING'
    }
  })
}

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

  let request: ReturnType<typeof bindGraphqlClient> | null = null
  let token: string | null = null
  let active = false
  let pending = Promise.resolve()

  async function discoveryRequest(client: ReturnType<typeof bindGraphqlClient>, document: string, token: string): Promise<void> {
    const result = await client(document, { token })
    if (result.errors?.length) throw new Error(result.errors[0].message)
  }

  async function refreshDevices(client: ReturnType<typeof bindGraphqlClient>, current: string): Promise<void> {
    const result = await client<{ nearbyDevices: DiscoveredDevice[]; pairingStates?: PairingState[] }>(nearbyDevicesGQL)
    if (result.errors?.length) throw new Error(result.errors[0].message)
    if (active && token === current) {
      devices.value = result.data.nearbyDevices
      pairingStates.clear()
      for (const state of result.data.pairingStates ?? []) applyPairingState(state)
    }
  }

  function enqueue(operation: () => Promise<void>): Promise<void> {
    const result = pending.then(operation)
    pending = result.catch(() => {})
    return result
  }

  async function acquire(): Promise<void> {
    const current = token
    const client = request
    if (!active || !current || !client) return
    await enqueue(async () => {
      if (!active || token !== current) return
      try {
        await discoveryRequest(client, acquireDiscoveryGQL, current)
        if (active && token === current) await refreshDevices(client, current)
      } catch (error) {
        if (active) status.value = DiscoveryStatus.NETWORK_ERROR
        console.error('discovery acquisition failed', error)
      }
    })
  }

  async function start(): Promise<void> {
    if (active) return
    request = bindGraphqlClient()
    token = randomUUID()
    active = true
    activeSessions.add(acquire)
    status.value = DiscoveryStatus.SEARCHING
    await acquire()
  }

  async function stop(): Promise<void> {
    const current = token
    const client = request
    if (!active || !current || !client) return
    active = false
    token = null
    activeSessions.delete(acquire)
    await enqueue(async () => {
      try { await discoveryRequest(client, releaseDiscoveryGQL, current) }
      catch (error) { console.error('discovery release failed', error) }
    })
    if (activeSessions.size === 0) {
      status.value = DiscoveryStatus.IDLE
      devices.value = []
      pairingStates.clear()
    }
  }

  function retry() {
    status.value = DiscoveryStatus.SEARCHING
    return active ? acquire() : start()
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
