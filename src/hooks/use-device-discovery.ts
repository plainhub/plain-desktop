import { ref } from 'vue'
import { acquireDiscoveryGQL, releaseDiscoveryGQL } from '@/lib/api/mutation'
import { bindGraphqlClient } from '@/lib/api/gql-client'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { nearbyDevicesGQL } from '@/lib/api/query'
import { randomUUID } from '@/lib/strutil'
import { useChatStore } from '@/stores/chat'
import { pairingViews } from '@/lib/device/pairing'
import { DeviceType, PeerStatus } from '@/lib/status'

export type DiscoveryMethod = 'LAN' | 'DNS_SD' | 'BLE' | 'QR'

export type DiscoveryDeviceStatus = 'PAIRING' | 'UNPAIRING' | PeerStatus

export interface DiscoveredDevice {
  id: string
  name: string
  ips: string[]
  bestIp?: string
  port: number
  remoteAccess?: boolean | null
  deviceType: DeviceType
  version: string
  platform: string
  lastSeen: string
  status: DiscoveryDeviceStatus
  discoveryMethods: DiscoveryMethod[]
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

function restoreDeviceStatus(deviceId: string) {
  const device = devices.value.find((d) => d.id === deviceId)
  if (device) device.status = useChatStore().findPeer(deviceId)?.status ?? PeerStatus.UNPAIRED
}

export function useDeviceDiscovery(bindClient = bindGraphqlClient, subscriptions = graphqlSubscriptions) {

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
    const result = await client<{ nearbyDevices: DiscoveredDevice[]; mdnsState: { scanning: boolean } }>(nearbyDevicesGQL)
    if (result.errors?.length) throw new Error(result.errors[0].message)
    if (active && token === current) {
      devices.value = result.data.nearbyDevices
      status.value = !result.data.mdnsState.scanning
        ? DiscoveryStatus.IDLE
        : devices.value.length ? DiscoveryStatus.OK : DiscoveryStatus.SEARCHING
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
  let stopSubscriptions: (() => void)[] = []

  async function start(): Promise<void> {
    if (active) return
    request = bindClient()
    token = randomUUID()
    active = true
    activeSessions.add(session)
    stopSubscriptions = [
      subscriptions.subscribe('nearbyDevicesChanged', refresh),
      subscriptions.subscribe('mdnsChanged', refresh),
      subscriptions.subscribe('pairingStarted', notice => {
        const device = devices.value.find(device => device.id === notice.deviceId)
        if (device) device.status = 'PAIRING'
      }),
      subscriptions.subscribe('pairingCompleted', notice => {
        const device = devices.value.find(device => device.id === notice.deviceId)
        if (device) device.status = PeerStatus.PAIRED
        useChatStore().fetchPeers()
      }),
      subscriptions.subscribe('pairingFailed', notice => restoreDeviceStatus(notice.deviceId)),
      subscriptions.subscribe('pairingCanceled', notice => restoreDeviceStatus(notice.deviceId)),
      subscriptions.onAttached(() => { void acquire() }),
    ]
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
    stopSubscriptions.forEach(stop => stop())
    stopSubscriptions = []
    await enqueue(async () => {
      try { await discoveryRequest(client, releaseDiscoveryGQL, current) }
      catch (error) { console.error('discovery release failed', error) }
    })
    if (activeSessions.size === 0) {
      status.value = DiscoveryStatus.IDLE
      devices.value = []
      pairingViews.clear()
    }
  }

  function retry() {
    status.value = DiscoveryStatus.SEARCHING
    return active ? acquire() : start()
  }

  async function openLanPermissionSettings() {
    if (__IS_TAURI__) {
      const { openUrl } = await import('@/lib/tauri-plugins')
      await openUrl('x-apple.systempreferences:').catch(async () => {
        await openUrl('https://support.apple.com/guide/mac-help/mchla4f49138/mac')
      })
      return
    }
    if (typeof window === 'undefined') return
    window.location.href = 'x-apple.systempreferences:'
  }

  return { devices, pairingViews, status, start, stop, retry, openLanPermissionSettings }
}
