<template>
  <v-modal width="560px" :escape-close="false" @close="handleClose">
    <template #headline>
      <template v-if="removeTarget">{{ $t('device_discovery.unpair_confirm', { name: removeTarget.name }) }}</template>
      <template v-else-if="step === 'discover'">{{ $t('nearby_devices') }}</template>
      <template v-else>
        <v-icon-button v-if="step === 'mode' || (selectedAttempt?.mode === 'control' && !loginForms.get(selectedAttempt.device.id)?.hasCancel)" :aria-label="$t('back')" @click="backToDevices"><i-material-symbols:arrow-back-rounded /></v-icon-button>
        <span>{{ $t('pairing_with', { name: selectedDevice?.name }) }}</span>
      </template>
    </template>
    <template #content>
      <template v-if="removeTarget">
        <div>{{ $t('device_discovery.unpair_hint') }}</div>
        <div v-if="removeTarget.id === currentClientId">{{ $t('device_discovery.unpair_current_hint') }}</div>
      </template>
      <NearbyPairOptions v-else-if="step === 'mode'" @cancel="backToDevices" @confirm="confirmPair" />
      <template v-else-if="selectedAttempt?.mode === 'chat'">
        <RequestApproval :code="selectedAttempt.pairing.state.value?.verificationCode" :hint="$t('pairing_compare_code')" @cancel="backToDevices">
          <v-text-button @click="showDiscovery">{{ $t('pair_another_device') }}</v-text-button>
        </RequestApproval>
      </template>
      <div v-else-if="step === 'discover'" class="nearby-sections">
        <section v-if="selfDevice || groups.mine.length">
          <h3 v-if="groups.mine.length" class="section-title">{{ $t('paired_devices') }}</h3>
          <ul class="card list-items">
            <VListItem v-if="selfDevice" :title="selfDevice.name" :subtitle="selfDevice.ips.join(', ')">
              <template #start><PeerIcon :device-type="selfDevice.deviceType" /></template>
              <template #title>
                <span>{{ selfDevice.name }}</span>
                <i-lucide:check v-if="localMode" role="img" :aria-label="$t('device_discovery.current')" />
                <span v-else class="status-badge on">{{ $t('device_discovery.local') }}</span>
              </template>
              <template v-if="!localMode" #end
                ><v-icon-button :aria-label="$t('switch')" :title="$t('switch')" @click="switchDevice('')"><i-lucide:arrow-left-right /></v-icon-button></template
              >
            </VListItem>
            <NearbyDeviceItem
              v-for="peer in groups.mine"
              :key="peer.id"
              :peer="peer"
              :device="deviceById.get(peer.id)"
              :pending="attempts.has(peer.id)"
              :current="peer.id === currentClientId"
              :full-control="canSwitchTo(peer)"
              :disabled="!!switchingPeer"
              :switching="switchingPeer === peer.id"
              @connect="connectPeer(peer, $event)"
              @cancel="cancelPair(peer.id)"
              @remove="confirmRemoval(peer)"
            />
          </ul>
        </section>
        <section class="discovery-section">
          <DeviceDiscoveryStatus :status="status" :show-searching="status === DiscoveryStatus.SEARCHING || status === DiscoveryStatus.OK" @retry="refresh" @open-settings="openLanPermissionSettings" />
          <MdnsFirewallFix :device-count="groups.nearby.length" @fixed="refresh" />
          <ul v-if="groups.nearby.length" class="card list-items">
            <NearbyDeviceItem
              v-for="device in groups.nearby"
              :key="device.id"
              :device="device"
              :pending="attempts.has(device.id)"
              @connect="selectDevice(device)"
              @cancel="cancelPair(device.id)"
            />
          </ul>
          <div v-if="bluetoothSupported && bluetoothStatus !== 'scanning'" class="subtle bluetooth-tip">
            <span role="status">{{ $t(`nearby_bluetooth_${bluetoothStatus}`) }}</span>
            <v-text-button v-if="bluetoothStatus === 'idle'" @click="enableBluetooth">
              {{ $t('nearby_bluetooth_enable') }}
            </v-text-button>
            <v-text-button v-if="bluetoothStatus === 'denied'" @click="openBluetoothSettings">
              {{ $t('nearby_bluetooth_allow_in_settings') }}
            </v-text-button>
          </div>
        </section>
        <QrPairPanel v-if="isLocalMode()" v-model:open="qrOpen" collapsible @paired="loadPeers().catch(reportFailure)" />
      </div>
      <template v-for="attempt in attempts.values()" :key="attempt.device.id">
        <DiscoveredDeviceLogin
          v-if="attempt.mode === 'control'"
          v-show="selectedAttempt === attempt"
          :ref="(view) => setLoginForm(attempt.device.id, view as LoginForm | null)"
          :device-id="attempt.device.id"
          :subscriptions="subscriptions"
          :managed-origin="origin"
          @success="onLoginSuccess(attempt.device.id)"
          @cancel="finishPairing(attempt.device.id)"
        >
          <v-text-button @click="showDiscovery">{{ $t('pair_another_device') }}</v-text-button>
        </DiscoveredDeviceLogin>
      </template>
    </template>
    <template v-if="removeTarget || step === 'discover'" #actions>
      <template v-if="removeTarget">
        <v-text-button :disabled="removing" @click="handleClose">{{ $t('cancel') }}</v-text-button>
        <v-filled-button class="danger" :loading="removing" @click="removeDevice(removeTarget)">{{ $t('unpair') }}</v-filled-button>
      </template>
      <v-outlined-button v-else :disabled="removing" @click="handleClose">{{ $t('close') }}</v-outlined-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { pairingErrorMessage } from '@/lib/api/pairing-error-code'
import { HttpErrorCode } from '@/lib/api/http-error-code'

import { computed, onBeforeUnmount, onMounted, ref, shallowReactive, watch } from 'vue'
import { popModal, pushModal } from '@/components/modal'
import toast from '@/components/toaster'
import { useI18n } from 'vue-i18n'
import DeviceDiscoveryStatus from '@/components/DeviceDiscoveryStatus.vue'
import MdnsFirewallFix from '@/components/MdnsFirewallFix.vue'
import QrPairPanel from '@/components/QrPairPanel.vue'
import RequestApproval from '@/components/RequestApproval.vue'
import NearbyPairOptions from './NearbyPairOptions.vue'
import NearbyDeviceItem from './NearbyDeviceItem.vue'
import DiscoveredDeviceLogin from '@/views/login/DiscoveredDeviceLogin.vue'
import { DiscoveryStatus, useDeviceDiscovery, type DiscoveredDevice } from '@/hooks/use-device-discovery'
import { GqlError } from '@/lib/api/gql-error'
import { bindGraphqlClient, gqlFetch, nativeGqlFetch } from '@/lib/api/gql-client'
import { managedGraphql } from '@/lib/api/managed-proxy'
import { SubscriptionClient, graphqlSubscriptions } from '@/lib/api/subscription-client'
import { openNative, openSubscriptionConnection } from '@/lib/api/subscription-connection'
import { subscriptionOperations } from '@/lib/api/subscription-operations'
import { peersGQL } from '@/lib/api/query'
import { cancelPairingGQL, unpairPeerGQL } from '@/lib/api/mutation'
import type { IPeer } from '@/lib/interfaces'
import { nearbyGroups } from '@/lib/device/nearby'
import { createPairing, pairingViews } from '@/lib/device/pairing'
import { preloadLoginPeers, findLoginPeer, loginPeers } from '@/lib/device/login-peers'
import { loadSelfDevice, type SelfDevice } from '@/lib/device/self-device'
import { getRemoteClientId } from '@/lib/device/client-id'
import { switchDevice } from '@/lib/device/switch-device'
import { isLocalMode } from '@/lib/device/local-mode'
import { remove as prefsRemove } from '@/lib/prefs/browser'
import { Channel, invoke, onNativeReload } from '@/lib/api/native-invoke'
import { useTempStore } from '@/stores/temp'
import { Capability } from '@/lib/data'
import { hasFeature } from '@/lib/feature'

const { origin = false } = defineProps<{ origin?: boolean }>()
const { t } = useI18n()
const request = __IS_TAURI__ ? nativeGqlFetch : bindGraphqlClient(origin)
const ownSubscriptions = __IS_TAURI__ || (origin && !!getRemoteClientId())
const subscriptions = ownSubscriptions
  ? new SubscriptionClient(__IS_TAURI__ ? openNative : callbacks => openSubscriptionConnection(callbacks, ''))
  : graphqlSubscriptions
const { devices, status, retry, start, stop, openLanPermissionSettings } = useDeviceDiscovery(() => request, subscriptions)
type BluetoothStatus = 'idle' | 'starting' | 'off' | 'denied' | 'unsupported' | 'scanning'
const bluetoothStatus = ref<BluetoothStatus>('idle')
const bluetoothSupported = __IS_TAURI__
  ? computed(() => hasFeature(Capability.BLE, useTempStore().app.capabilities))
  : false
let bluetoothStates: Channel<BluetoothStatus> | undefined

async function enableBluetooth() {
  if (bluetoothStates) return
  bluetoothStatus.value = 'starting'
  const states = new Channel<BluetoothStatus>((state) => {
    if (bluetoothStates === states) bluetoothStatus.value = state
  })
  bluetoothStates = states
  try {
    await invoke('nearby_bluetooth_enable', { states })
  } catch (reason) {
    if (bluetoothStates !== states) return
    console.error('Could not enable nearby Bluetooth', reason)
    bluetoothStates = undefined
    bluetoothStatus.value = 'unsupported'
  }
}

function stopBluetooth() {
  const states = bluetoothStates
  bluetoothStates = undefined
  if (states) void invoke('nearby_bluetooth_stop', { scope: states.id }).catch(console.error)
}

function openBluetoothSettings() {
  void invoke('nearby_bluetooth_open_settings').catch(reportFailure)
}

const stopNativeReload = __IS_TAURI__ ? onNativeReload(stopBluetooth) : undefined
type PairingAttempt = { device: DiscoveredDevice; stop: () => void } & (
  { mode: 'chat'; pairing: ReturnType<typeof createPairing> } | { mode: 'control' }
)
type LoginForm = InstanceType<typeof DiscoveredDeviceLogin>
const attempts = shallowReactive(new Map<string, PairingAttempt>())
const loginForms = shallowReactive(new Map<string, LoginForm>())
const peers = ref<IPeer[]>([])
const selfDevice = ref<SelfDevice | null>(null)
const selectedDevice = ref<DiscoveredDevice | null>(null)
const qrOpen = ref(false)
const step = ref<'discover' | 'mode' | 'pairing'>('discover')
const selectedAttempt = computed(() => step.value === 'pairing' && selectedDevice.value ? attempts.get(selectedDevice.value.id) : undefined)
const switchingPeer = ref('')
const removeTarget = ref<IPeer | null>(null)
const removing = ref(false)
const currentClientId = computed(() => getRemoteClientId())
const localMode = computed(() => __IS_TAURI__ ? isLocalMode() : !currentClientId.value)
const deviceById = computed(() => new Map([
  ...Array.from(attempts, ([id, attempt]) => [id, attempt.device] as const),
  ...devices.value.map(device => [device.id, device] as const),
]))
const groups = computed(() => nearbyGroups(peers.value, [...deviceById.value.values()], selfDevice.value?.id,
  new Set(__IS_TAURI__ ? loginPeers.value.map(peer => peer.id) : peers.value.filter(peer => peer.managed).map(peer => peer.id)),
))
let active = true

async function loadPeers() {
  const [result] = await Promise.all([request<{ peers: IPeer[] }>(peersGQL), preloadLoginPeers()])
  if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
  if (active) peers.value = result.data.peers
}
function reportFailure(reason: unknown) {
  console.error('Nearby devices request failed', reason)
  if (active) toast(t(HttpErrorCode.NetworkError), 'error')
}
const stopPeers = subscriptions.subscribe('peersChanged', () => loadPeers().catch(reportFailure))
const stopAttached = subscriptions.onAttached(() => {
  void loadPeers().catch(reportFailure)
})

function selectDevice(device: DiscoveredDevice) {
  selectedDevice.value = device
  if (device.remoteAccess === true) step.value = 'mode'
  else void startChat(device)
}
function canSwitchTo(peer: IPeer) {
  return __IS_TAURI__ ? !!findLoginPeer(peer.id) : (origin || !currentClientId.value) && peer.managed === true
}
function connectPeer(peer: IPeer, action: string) {
  if (action === 'switch') {
    void switchTo(peer)
    return
  }
  const device = deviceById.value.get(peer.id)
  if (device) {
    selectedDevice.value = device
    void confirmPair('control')
  }
}
async function confirmPair(mode: 'chat' | 'control') {
  const device = selectedDevice.value
  if (!device) return
  if (mode === 'chat') {
    await startChat(device)
    return
  }
  if (!device.bestIp && !device.ips.length) return
  const stop = watch(() => pairingViews.get(device.id)?.phase, phase => {
    if (selectedAttempt.value?.device.id === device.id) return
    if (phase === 'CANCELED' || phase === 'FAILED') {
      finishPairing(device.id)
      if (phase === 'FAILED') toast(t(pairingErrorMessage(pairingViews.get(device.id)?.error)), 'error')
    }
  })
  attempts.set(device.id, { mode, device, stop })
  step.value = 'pairing'
}
async function startChat(device: DiscoveredDevice) {
  const pairing = createPairing(subscriptions, request)
  const stop = watch(pairing.state, state => {
    if (!state || !['SUCCEEDED', 'FAILED', 'CANCELED'].includes(state.phase)) return
    finishPairing(device.id)
    if (state.phase === 'FAILED') toast(t(pairingErrorMessage(state.error)), 'error')
    if (state.phase === 'SUCCEEDED') void loadPeers().catch(reportFailure)
  })
  const attempt: PairingAttempt = { mode: 'chat', device, pairing, stop: () => { stop(); pairing.dispose() } }
  attempts.set(device.id, attempt)
  step.value = 'pairing'
  try {
    await pairing.start({ peerId: device.id, mode: 'CHAT' })
  } catch (reason) {
    if (attempts.get(device.id) === attempt) {
      finishPairing(device.id)
      reportFailure(reason)
    }
  }
}
function setLoginForm(id: string, view: LoginForm | null) {
  if (view) loginForms.set(id, view)
  else loginForms.delete(id)
}
async function cancelPair(deviceId: string) {
  const attempt = attempts.get(deviceId)
  if (attempt?.mode === 'control') {
    loginForms.get(deviceId)?.cancel()
    return
  }
  try {
    if (attempt?.mode === 'chat') await attempt.pairing.cancel(deviceId)
    else {
      const result = await request(cancelPairingGQL, { peerId: deviceId })
      if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
    }
  } catch (reason) {
    reportFailure(reason)
  }
}
async function backToDevices() {
  if (selectedAttempt.value) await cancelPair(selectedAttempt.value.device.id)
  else showDiscovery()
}

function showDiscovery() {
  step.value = 'discover'
  selectedDevice.value = null
}
function finishPairing(id: string) {
  if (selectedAttempt.value?.device.id === id) showDiscovery()
  attempts.get(id)?.stop()
  attempts.delete(id)
}
async function onLoginSuccess(id: string) {
  finishPairing(id)
  await loadPeers().catch(reportFailure)
}
async function switchTo(peer: IPeer) {
  const accessOff = deviceById.value.get(peer.id)?.remoteAccess === false
  if (accessOff || !peer.online) {
    toast(peer.name + ': ' + t(peer.online ? 'remote_access_off' : 'offline'))
    return
  }
  if (__IS_TAURI__) {
    await preloadLoginPeers()
    if (canSwitchTo(peer)) switchDevice(peer.id)
    return
  }
  if (!canSwitchTo(peer) || switchingPeer.value || removeTarget.value) return
  switchingPeer.value = peer.id
  const document = 'query { app { clientId } }'
  try {
    try {
      const result = await managedGraphql<{ app?: { clientId: string } }>(peer.id, document, undefined, 8000)
      if (!active) return
      if (!result.errors?.length && result.data?.app?.clientId === peer.id) {
        switchDevice(peer.id)
        return
      }
    } catch (reason) {
      if (!active) return
      if (!(reason instanceof GqlError) || (reason.status !== undefined && reason.status !== 401 && reason.status !== 502)) {
        reportFailure(reason)
        return
      }
      if (reason.status !== 502) {
        try {
          const origin = await gqlFetch<{ app?: { clientId: string } }>(document, undefined, { origin: true, timeout: 8000 })
          if (!origin.data?.app || origin.errors?.length) throw reason
        } catch (originError) {
          if (active) reportFailure(originError)
          return
        }
      }
    }
    await showSwitchFailure(peer)
  } finally {
    switchingPeer.value = ''
  }
}
async function showSwitchFailure(peer: IPeer) {
  const { default: dialog } = await import('@/components/ManagedPeerUnavailable.vue')
  if (active) await pushModal(dialog, { peerName: peer.name, originName: selfDevice.value?.name })
}
function confirmRemoval(peer: IPeer) {
  if (removing.value || switchingPeer.value) return
  removeTarget.value = peer
}
async function removeDevice(peer: IPeer) {
  if (removing.value || removeTarget.value?.id !== peer.id) return
  removing.value = true
  try {
    const result = await request<{ unpairPeer: boolean }>(unpairPeerGQL, { peerId: peer.id })
    if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
    if (!result.data.unpairPeer) throw new Error('Device removal failed')
    prefsRemove(`main_state:${peer.id}`)
    if (peer.id === getRemoteClientId()) switchDevice('')
    else {
      removeTarget.value = null
      peers.value = peers.value.filter(item => item.id !== peer.id)
      await loadPeers()
    }
  } catch (reason) {
    reportFailure(reason)
  } finally {
    removing.value = false
  }
}
async function refresh() {
  try {
    await Promise.all([retry(), loadPeers()])
  } catch (reason) {
    reportFailure(reason)
  }
}
async function handleClose() {
  if (removeTarget.value) {
    if (!removing.value) {
      removeTarget.value = null
    }
    return
  }
  if (step.value !== 'discover') {
    await backToDevices()
    return
  }
  stopBluetooth()
  await stop()
  await popModal()
}
defineExpose({ onEscape: handleClose })

onMounted(async () => {
  if (ownSubscriptions) {
    subscriptions.setOperations(subscriptionOperations)
    subscriptions.start()
  }
  try {
    await Promise.all([
      start(),
      loadPeers(),
      (__IS_TAURI__ || origin)
        ? loadSelfDevice(request).then((value) => {
            if (active) selfDevice.value = value
          })
        : Promise.resolve(),
    ])
  } catch (reason) {
    reportFailure(reason)
  }
})
onBeforeUnmount(() => {
  active = false
  stopBluetooth()
  stopNativeReload?.()
  stopPeers()
  stopAttached()
  for (const attempt of attempts.values()) attempt.stop()
  void stop().finally(() => {
    if (ownSubscriptions) subscriptions.stop()
  })
})
</script>

<style scoped lang="scss">
.nearby-sections {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.discovery-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.bluetooth-tip {
  font-size: 0.8125rem;
}
.bluetooth-tip .v-text-button {
  font-size: inherit;
}
</style>
