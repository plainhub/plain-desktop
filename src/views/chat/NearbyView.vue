<template>
  <v-modal width="560px" @close="handleClose">
    <template #headline>
      <template v-if="removeTarget">{{ $t('device_discovery.unpair_confirm', { name: removeTarget.name }) }}</template>
      <template v-else-if="step === 'discover'">{{ $t(switchOnly ? 'device_discovery.change_device' : 'nearby_devices') }}</template>
      <template v-else>
        <v-icon-button :aria-label="$t('back')" @click="backToDevices"><i-material-symbols:arrow-back-rounded /></v-icon-button>
        <span>{{ $t('pairing_with', { name: selectedDevice?.name }) }}</span>
      </template>
    </template>
    <template #content>
      <section v-if="switchOnly" class="discovery-section">
        <DeviceDiscoveryStatus :status="status" @retry="refresh" />
        <ul v-if="switchTargets.length" class="card list-items">
          <VListItem v-for="{ device, href } in switchTargets" :key="device.id" :title="device.name" :subtitle="device.bestIp || device.ips.join(', ')">
            <template #start><PeerIcon :device-type="device.deviceType" /></template>
            <template #end><a class="web-switch-link" :href="href" referrerpolicy="no-referrer" @click.stop>{{ $t('switch') }}</a></template>
          </VListItem>
        </ul>
        <span v-else-if="status === DiscoveryStatus.OK">{{ $t('no_devices_found') }}</span>
        <div v-if="error" class="nearby-error" role="alert">
          <span>{{ $t(error) }}</span>
          <v-outlined-button @click="refresh">{{ $t('retry') }}</v-outlined-button>
        </div>
      </section>
      <template v-else-if="removeTarget">
        <div>{{ $t('device_discovery.unpair_hint') }}</div>
        <div v-if="removeTarget.id === currentClientId">{{ $t('device_discovery.unpair_current_hint') }}</div>
        <div v-if="error" class="nearby-error" role="alert">{{ $t(error) }}</div>
      </template>
      <NearbyPairOptions v-else-if="step === 'mode'" @cancel="backToDevices" @confirm="confirmPair" />
      <DiscoveredDeviceLogin v-else-if="step === 'login' && loginDevice" :device="loginDevice" :subscriptions="subscriptions" @success="onLoginSuccess" @cancel="backToDevices" />
      <div v-else class="nearby-sections">
        <section v-if="selfDevice || groups.mine.length">
          <h3 v-if="groups.mine.length" class="section-title">{{ $t('paired_devices') }}</h3>
          <ul class="card list-items">
            <VListItem v-if="selfDevice" :title="selfDevice.name" :subtitle="selfDevice.ips.join(', ')">
              <template #start><PeerIcon :device-type="selfDevice.deviceType" /></template>
              <template #title>
                <span>{{ selfDevice.name }}</span>
                <span class="status-badge on">{{ $t(localMode ? 'device_discovery.current' : 'device_discovery.local') }}</span>
              </template>
              <template v-if="!localMode" #end
                ><v-outlined-button @click="switchToLocal">{{ $t('switch') }}</v-outlined-button></template
              >
            </VListItem>
            <NearbyDeviceItem
              v-for="peer in groups.mine"
              :key="peer.id"
              :peer="peer"
              :device="deviceById.get(peer.id)"
              :current="peer.id === currentClientId"
              :full-control="!!findLoginPeer(peer.id)"
              :pairing="activePairId === peer.id || deviceById.get(peer.id)?.status === 'PAIRING'"
              :disabled="!!activePairId"
              @connect="connectPeer(peer, $event)"
              @cancel="cancelPair(peer.id)"
              @remove="confirmRemoval(peer)"
            />
          </ul>
        </section>
        <section class="discovery-section">
          <DeviceDiscoveryStatus :status="status" @retry="refresh" @open-settings="openLanPermissionSettings" />
          <MdnsFirewallFix :device-count="groups.nearby.length" @fixed="refresh" />
          <ul v-if="groups.nearby.length" class="card list-items">
            <NearbyDeviceItem
              v-for="device in groups.nearby"
              :key="device.id"
              :device="device"
              :pairing="activePairId === device.id || device.status === 'PAIRING'"
              :disabled="!!activePairId"
              @connect="selectDevice(device)"
              @cancel="cancelPair(device.id)"
            />
          </ul>
        </section>
        <div v-for="state in verificationStates" :key="state.deviceId" class="pairing-verification">
          <p>{{ deviceById.get(state.deviceId!)?.name ?? peers.find((peer) => peer.id === state.deviceId)?.name }} · {{ $t('waiting_for_confirmation') }}</p>
          <p v-if="state.identityChanged" role="alert">{{ $t('pairing_identity_changed') }}</p>
          <RequestVerificationCode :code="state.verificationCode!" :hint="$t('pairing_compare_code')" />
          <v-filled-button v-if="state.phase === 'TRUST_REQUIRED'" @click="pairing.confirm(state.deviceId).catch(reportFailure)">{{ $t(state.identityChanged ? 'pairing_trust' : 'confirm') }}</v-filled-button>
        </div>
        <div v-if="error" class="nearby-error" role="alert">
          <span>{{ $t(error) }}</span
          ><v-outlined-button @click="refresh">{{ $t('retry') }}</v-outlined-button>
        </div>
      </div>
    </template>
    <template v-if="removeTarget || step === 'discover'" #actions>
      <template v-if="removeTarget">
        <v-text-button :disabled="removing" @click="handleClose">{{ $t('cancel') }}</v-text-button>
        <v-filled-button class="danger" :loading="removing" @click="removeDevice(removeTarget)">{{ $t('unpair') }}</v-filled-button>
      </template>
      <v-outlined-button v-else @click="handleClose">{{ $t('close') }}</v-outlined-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { popModal } from '@/components/modal'
import DeviceDiscoveryStatus from '@/components/DeviceDiscoveryStatus.vue'
import MdnsFirewallFix from '@/components/MdnsFirewallFix.vue'
import RequestVerificationCode from '@/components/RequestVerificationCode.vue'
import NearbyPairOptions from './NearbyPairOptions.vue'
import NearbyDeviceItem from './NearbyDeviceItem.vue'
import DiscoveredDeviceLogin from '@/views/login/DiscoveredDeviceLogin.vue'
import { DiscoveryStatus, useDeviceDiscovery, type DiscoveredDevice } from '@/hooks/use-device-discovery'
import { bindGraphqlClient, nativeGqlFetch } from '@/lib/api/gql-client'
import { SubscriptionClient, graphqlSubscriptions } from '@/lib/api/subscription-client'
import { openNative } from '@/lib/api/subscription-connection'
import { subscriptionOperations } from '@/lib/api/subscription-operations'
import { peersGQL } from '@/lib/api/query'
import { unpairPeerGQL } from '@/lib/api/mutation'
import type { PendingLoginDevice } from '@/lib/api/api'
import type { IPeer } from '@/lib/interfaces'
import { nearbyGroups, webSwitchDeviceUrl } from '@/lib/device/nearby'
import { createPairing } from '@/lib/device/pairing'
import { preloadLoginPeers, findLoginPeer, loginPeers } from '@/lib/device/login-peers'
import { loadSelfDevice, type SelfDevice } from '@/lib/device/self-device'
import { clearRemoteClientId, getRemoteClientId, setRemoteClientId } from '@/lib/device/client-id'
import { isLocalMode } from '@/lib/device/local-mode'
import { remove as prefsRemove } from '@/lib/prefs/browser'

const { switchOnly = false } = defineProps<{ switchOnly?: boolean }>()
const request = __IS_TAURI__ ? nativeGqlFetch : bindGraphqlClient()
const subscriptions = __IS_TAURI__ ? new SubscriptionClient(openNative) : graphqlSubscriptions
const { devices, pairingViews, status, retry, start, stop, openLanPermissionSettings } = useDeviceDiscovery(() => request, subscriptions)
const pairing = createPairing(subscriptions, request)
const peers = ref<IPeer[]>([])
const selfDevice = ref<SelfDevice | null>(null)
const selectedDevice = ref<DiscoveredDevice | null>(null)
const loginDevice = ref<(PendingLoginDevice & { id: string }) | null>(null)
const step = ref<'discover' | 'mode' | 'login'>('discover')
const activePairId = ref('')
const removeTarget = ref<IPeer | null>(null)
const removing = ref(false)
const error = ref('')
const currentClientId = computed(() => getRemoteClientId())
const switchTargets = computed(() => devices.value.flatMap(device => {
  if (device.id === currentClientId.value) return []
  const href = webSwitchDeviceUrl(device)
  return href && new URL(href).origin !== window.location.origin ? [{ device, href }] : []
}))
const localMode = computed(() => isLocalMode())
const deviceById = computed(() => new Map(devices.value.map((device) => [device.id, device])))
const groups = computed(() => nearbyGroups(peers.value, devices.value, selfDevice.value?.id, new Set(loginPeers.value.map((peer) => peer.id))))
const verificationStates = computed(() => Array.from(pairingViews.values()).filter((state) => state.verificationCode && !['SUCCEEDED', 'FAILED', 'CANCELED'].includes(state.phase)))
let active = true

async function loadPeers() {
  if (switchOnly) return
  const [result] = await Promise.all([request<{ peers: IPeer[] }>(peersGQL), preloadLoginPeers()])
  if (result.errors?.length) throw new Error(result.errors[0].message)
  if (active) peers.value = result.data.peers
}
function reportFailure(reason: unknown) {
  console.error('Nearby devices request failed', reason)
  error.value = 'network_error'
}
const stopPeers = subscriptions.subscribe('peersChanged', () => loadPeers().catch(reportFailure))
const stopAttached = subscriptions.onAttached(() => {
  void loadPeers().catch(reportFailure)
})

watch(pairing.state, async (state) => {
  if (!state) return
  if (state.phase === 'FAILED') error.value = 'pair_failed'
  if (['SUCCEEDED', 'FAILED', 'CANCELED'].includes(state.phase)) {
    activePairId.value = ''
    if (state.phase === 'SUCCEEDED') {
      try {
        await loadPeers()
      } catch (reason) {
        reportFailure(reason)
      }
    }
  }
})

function selectDevice(device: DiscoveredDevice) {
  selectedDevice.value = device
  if (device.remoteAccess === true) step.value = 'mode'
  else void startChat(device)
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
    step.value = 'discover'
    await startChat(device)
    return
  }
  const ip = device.bestIp || device.ips[0]
  if (!ip) return
  loginDevice.value = { id: device.id, name: device.name, deviceType: device.deviceType, host: `${ip.includes(':') ? `[${ip}]` : ip}:${device.port}` }
  step.value = 'login'
}
async function startChat(device: DiscoveredDevice) {
  activePairId.value = device.id
  error.value = ''
  try {
    await pairing.start({ peerId: device.id, mode: 'CHAT' })
  } catch (reason) {
    activePairId.value = ''
    reportFailure(reason)
  }
}
async function cancelPair(deviceId: string) {
  try {
    await pairing.cancel(deviceId)
    if (activePairId.value === deviceId) activePairId.value = ''
  } catch (reason) {
    reportFailure(reason)
  }
}
function backToDevices() {
  step.value = 'discover'
  selectedDevice.value = null
  loginDevice.value = null
}
async function onLoginSuccess() {
  await stop()
  await popModal()
  window.location.href = '/'
}
async function switchTo(peer: IPeer) {
  await preloadLoginPeers()
  setRemoteClientId(peer.id)
  window.location.href = '/'
}
function switchToLocal() {
  clearRemoteClientId()
  window.location.href = '/'
}
function confirmRemoval(peer: IPeer) {
  error.value = ''
  removeTarget.value = peer
}
async function removeDevice(peer: IPeer) {
  removing.value = true
  error.value = ''
  try {
    const result = await request<{ unpairPeer: boolean }>(unpairPeerGQL, { id: peer.id })
    if (result.errors?.length) throw new Error(result.errors[0].message)
    if (!result.data.unpairPeer) throw new Error('Device removal failed')
    prefsRemove(`main_state:${peer.id}`)
    if (peer.id === getRemoteClientId()) switchToLocal()
    else {
      await loadPeers()
      removeTarget.value = null
    }
  } catch (reason) {
    reportFailure(reason)
  } finally {
    removing.value = false
  }
}
async function refresh() {
  error.value = ''
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
      error.value = ''
    }
    return
  }
  if (step.value !== 'discover') {
    backToDevices()
    return
  }
  await stop()
  await popModal()
}
onMounted(async () => {
  if (__IS_TAURI__) {
    subscriptions.setOperations(subscriptionOperations)
    subscriptions.start()
  }
  try {
    await Promise.all([
      start(),
      loadPeers(),
      __IS_TAURI__
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
  stopPeers()
  stopAttached()
  pairing.dispose()
  void stop().finally(() => {
    if (__IS_TAURI__) subscriptions.stop()
  })
})
</script>

<style scoped lang="scss">
.web-switch-link {
  color: var(--md-sys-color-primary);
  padding: 8px 12px;
  border: 1px solid var(--md-sys-color-outline);
  border-radius: 999px;
  text-decoration: none;
}
.web-switch-link:hover {
  background: var(--md-sys-color-surface-container-high);
}

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
.nearby-error {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  color: var(--md-sys-color-error);
}
</style>
