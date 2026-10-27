import { ref } from 'vue'
import { acquireDiscoveryGQL, releaseDiscoveryGQL } from '@/lib/api/mutation'
import { bindGraphqlClient } from '@/lib/api/gql-client'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
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

const activeSessions = new Set<{ acquire: () => Promise<void>; refresh: () => Promise<void> }>()
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

  const refresh = () => { for (const session of activeSessions) void session.refresh() }
  graphqlSubscriptions.subscribe('nearbyDevicesChanged', refresh)
  graphqlSubscriptions.subscribe('mdnsChanged', refresh)
  graphqlSubscriptions.subscribe('pairingChanged', refresh)
  graphqlSubscriptions.onAttached(() => { for (const session of activeSessions) void session.acquire() })

  emitter.on('PAIRING_SUCCESS', (result: PairingResult) => {
    const device = devices.value.find((d) => d.id === result.deviceId)
    if (device) {
      device.status = PeerStatus.PAIRED
    }
    useChatStore().fetchPeers()
  })

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
  let refreshQueued = false
  let refreshAgain = false

  async function discoveryRequest(client: ReturnType<typeof bindGraphqlClient>, document: string, token: string): Promise<void> {
    const result = await client(document, { token })
    if (result.errors?.length) throw new Error(result.errors[0].message)
  }

  async function refreshDevices(client: ReturnType<typeof bindGraphqlClient>, current: string): Promise<void> {
    const result = await client<{ nearbyDevices: DiscoveredDevice[]; pairingStates?: PairingState[]; mdnsState: { scanning: boolean; receiver: boolean } }>(nearbyDevicesGQL)
    if (result.errors?.length) throw new Error(result.errors[0].message)
    if (active && token === current) {
      devices.value = result.data.nearbyDevices
      status.value = !result.data.mdnsState.scanning
        ? DiscoveryStatus.IDLE
        : devices.value.length ? DiscoveryStatus.OK : DiscoveryStatus.SEARCHING
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

  async function refresh(): Promise<void> {
    refreshAgain = true
    if (refreshQueued) return
    refreshQueued = true
    try {
      while (refreshAgain && active) {
        refreshAgain = false
        const current = token
        const client = request
        if (!current || !client) return
        await enqueue(async () => {
          if (!active || token !== current) return
          try { await refreshDevices(client, current) }
          catch (error) { console.error('discovery refresh failed', error) }
        })
      }
    } finally {
      refreshQueued = false
    }
  }

  const session = { acquire, refresh }

  async function start(): Promise<void> {
    if (active) return
    request = bindGraphqlClient()
    token = randomUUID()
    active = true
    activeSessions.add(session)
    status.value = DiscoveryStatus.SEARCHING
    await acquire()
  }

  async function stop(): Promise<void> {
    const current = token
    const client = request
    if (!active || !current || !client) return
    active = false
    token = null
    activeSessions.delete(session)
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
