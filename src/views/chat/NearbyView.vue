<template>
  <v-modal width="560px" @close="handleClose">
    <template #headline>
      <template v-if="removeTarget && !switchOnly">{{ $t('device_discovery.unpair_confirm', { name: removeTarget.name }) }}</template>
      <template v-else-if="step === 'discover'">{{ $t(switchOnly ? 'device_discovery.change_device' : 'nearby_devices') }}</template>
      <template v-else>
        <v-icon-button :aria-label="$t('back')" @click="backToDevices"><i-material-symbols:arrow-back-rounded /></v-icon-button>
        <span>{{ $t('pairing_with', { name: selectedDevice?.name }) }}</span>
      </template>
    </template>
    <template #content>
      <template v-if="removeTarget && !switchOnly">
        <div>{{ $t('device_discovery.unpair_hint') }}</div>
        <div v-if="removeTarget.id === currentClientId">{{ $t('device_discovery.unpair_current_hint') }}</div>
        <div v-if="error" class="nearby-error" role="alert">{{ $t(error) }}</div>
      </template>
      <NearbyPairOptions v-else-if="step === 'mode'" @cancel="backToDevices" @confirm="confirmPair" />
      <DiscoveredDeviceLogin v-else-if="step === 'login' && loginDevice" :device="loginDevice" :subscriptions="subscriptions" :managed-origin="useOrigin" @success="onLoginSuccess" @cancel="backToDevices" />
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
            <template v-for="peer in groups.mine" :key="peer.id">
              <NearbyDeviceItem
                :peer="peer"
                :device="deviceById.get(peer.id)"
                :current="peer.id === currentClientId"
                :full-control="canSwitchTo(peer)"
                :switch-only="switchOnly"
                :pairing="activePairId === peer.id || deviceById.get(peer.id)?.status === 'PAIRING'"
                :disabled="!!(activePairId || switchingPeer || (switchOnly && removeTarget))"
                :switching="switchingPeer === peer.id"
                @connect="connectPeer(peer, $event)"
                @cancel="cancelPair(peer.id)"
                @remove="confirmRemoval(peer)"
              />
              <li v-if="switchOnly && removeTarget?.id === peer.id" class="unpair-confirmation">
                <InlineDeleteConfirm
                  :name="peer.name" :loading="removing" message-key="device_discovery.unpair_confirm"
                  @cancel="handleClose" @confirm="removeDevice(peer)"
                >
                  <template #details>
                    <div>{{ $t('device_discovery.unpair_hint') }}</div>
                    <div v-if="peer.id === currentClientId">{{ $t('device_discovery.unpair_current_hint') }}</div>
                  </template>
                </InlineDeleteConfirm>
              </li>
            </template>
          </ul>
        </section>
        <section v-if="!switchOnly" class="discovery-section">
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
    <template v-if="(removeTarget && !switchOnly) || step === 'discover'" #actions>
      <template v-if="removeTarget && !switchOnly">
        <v-text-button :disabled="removing" @click="handleClose">{{ $t('cancel') }}</v-text-button>
        <v-filled-button class="danger" :loading="removing" @click="removeDevice(removeTarget)">{{ $t('unpair') }}</v-filled-button>
      </template>
      <v-outlined-button v-else :disabled="removing" @click="handleClose">{{ $t('close') }}</v-outlined-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { HttpErrorCode } from '@/lib/api/http-error-code'

import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { popModal, pushModal } from '@/components/modal'
import DeviceDiscoveryStatus from '@/components/DeviceDiscoveryStatus.vue'
import MdnsFirewallFix from '@/components/MdnsFirewallFix.vue'
import RequestVerificationCode from '@/components/RequestVerificationCode.vue'
import NearbyPairOptions from './NearbyPairOptions.vue'
import NearbyDeviceItem from './NearbyDeviceItem.vue'
import DiscoveredDeviceLogin from '@/views/login/DiscoveredDeviceLogin.vue'
import { useDeviceDiscovery, type DiscoveredDevice } from '@/hooks/use-device-discovery'
import { GqlError } from '@/lib/api/gql-error'
import { bindGraphqlClient, gqlFetch, nativeGqlFetch } from '@/lib/api/gql-client'
import { managedGraphql } from '@/lib/api/managed-proxy'
import { SubscriptionClient, graphqlSubscriptions } from '@/lib/api/subscription-client'
import { openNative, openSubscriptionConnection } from '@/lib/api/subscription-connection'
import { subscriptionOperations } from '@/lib/api/subscription-operations'
import { peersGQL } from '@/lib/api/query'
import { unpairPeerGQL } from '@/lib/api/mutation'
import type { PendingLoginDevice } from '@/lib/api/api'
import type { IPeer } from '@/lib/interfaces'
import { nearbyGroups } from '@/lib/device/nearby'
import { createPairing } from '@/lib/device/pairing'
import { preloadLoginPeers, findLoginPeer, loginPeers } from '@/lib/device/login-peers'
import { loadSelfDevice, type SelfDevice } from '@/lib/device/self-device'
import { getRemoteClientId } from '@/lib/device/client-id'
import { switchDevice } from '@/lib/device/switch-device'
import { isLocalMode } from '@/lib/device/local-mode'
import { remove as prefsRemove } from '@/lib/prefs/browser'

const { switchOnly = false, origin = false } = defineProps<{ switchOnly?: boolean; origin?: boolean }>()
const useOrigin = switchOnly || origin
const request = __IS_TAURI__ ? nativeGqlFetch : bindGraphqlClient(useOrigin)
const ownSubscriptions = __IS_TAURI__ || (useOrigin && !!getRemoteClientId())
const subscriptions = ownSubscriptions
  ? new SubscriptionClient(__IS_TAURI__ ? openNative : callbacks => openSubscriptionConnection(callbacks, ''))
  : graphqlSubscriptions
const { devices, pairingViews, status, retry, start, stop, openLanPermissionSettings } = useDeviceDiscovery(() => request, subscriptions)
const pairing = createPairing(subscriptions, request)
const peers = ref<IPeer[]>([])
const selfDevice = ref<SelfDevice | null>(null)
const selectedDevice = ref<DiscoveredDevice | null>(null)
const loginDevice = ref<(PendingLoginDevice & { id: string }) | null>(null)
const step = ref<'discover' | 'mode' | 'login'>('discover')
const activePairId = ref('')
const switchingPeer = ref('')
const removeTarget = ref<IPeer | null>(null)
const removing = ref(false)
const error = ref('')
const currentClientId = computed(() => getRemoteClientId())
const localMode = computed(() => __IS_TAURI__ ? isLocalMode() : !currentClientId.value)
const deviceById = computed(() => new Map(devices.value.map((device) => [device.id, device])))
const groups = computed(() => nearbyGroups(switchOnly ? peers.value.filter(peer => peer.managed) : peers.value, devices.value, selfDevice.value?.id,
  new Set(__IS_TAURI__ ? loginPeers.value.map(peer => peer.id) : peers.value.filter(peer => peer.managed).map(peer => peer.id)),
))
const verificationStates = computed(() => Array.from(pairingViews.values()).filter((state) => state.verificationCode && !['SUCCEEDED', 'FAILED', 'CANCELED'].includes(state.phase)))
let active = true

async function loadPeers() {
  const [result] = await Promise.all([request<{ peers: IPeer[] }>(peersGQL), preloadLoginPeers()])
  if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
  if (active) peers.value = result.data.peers
}
function reportFailure(reason: unknown) {
  console.error('Nearby devices request failed', reason)
  error.value = HttpErrorCode.NetworkError
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
function canSwitchTo(peer: IPeer) {
  return __IS_TAURI__ ? !!findLoginPeer(peer.id) : switchOnly && peer.managed === true
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
  const device = loginDevice.value
  await stop()
  await popModal()
  if (!__IS_TAURI__ && origin && device) switchDevice(device.id)
  else window.location.href = '/'
}
async function switchTo(peer: IPeer) {
  if (__IS_TAURI__) {
    await preloadLoginPeers()
    if (canSwitchTo(peer)) switchDevice(peer.id)
    return
  }
  if (!canSwitchTo(peer) || switchingPeer.value || removeTarget.value) return
  switchingPeer.value = peer.id
  error.value = ''
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
function switchToLocal() {
  switchDevice('')
}
function confirmRemoval(peer: IPeer) {
  if (removing.value || switchingPeer.value) return
  error.value = ''
  removeTarget.value = peer
}
async function removeDevice(peer: IPeer) {
  if (removing.value || removeTarget.value?.id !== peer.id) return
  removing.value = true
  error.value = ''
  try {
    const result = await request<{ unpairPeer: boolean }>(unpairPeerGQL, { peerId: peer.id })
    if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
    if (!result.data.unpairPeer) throw new Error('Device removal failed')
    prefsRemove(`main_state:${peer.id}`)
    if (peer.id === getRemoteClientId()) switchToLocal()
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
  if (ownSubscriptions) {
    subscriptions.setOperations(subscriptionOperations)
    subscriptions.start()
  }
  try {
    await Promise.all([
      switchOnly ? Promise.resolve() : start(),
      loadPeers(),
      (__IS_TAURI__ || useOrigin)
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
.unpair-confirmation {
  list-style: none;
}
.nearby-error {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  color: var(--md-sys-color-error);
}
</style>
