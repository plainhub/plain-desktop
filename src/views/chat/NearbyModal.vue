<template>
  <v-modal width="480px" @close="handleClose">
    <template #headline>
      <v-circular-progress indeterminate class="sm" aria-label="scanning" />
      <span>{{ $t('device_discovery.searching') }}</span>
    </template>
    <template #content>
      <div v-if="discoveredDevices.length === 0" class="nearby-empty">
        <MdnsFirewallFix :device-count="discoveredDevices.length" @fixed="retry" />
      </div>

      <ul v-else class="card list-items">
        <VListItem v-for="d in discoveredDevices" :key="d.id" :subtitle="d.ips.join(', ')">
          <template #title>
            <span>{{ d.name }}</span>
            <i-lucide:bluetooth
v-if="d.discoveryMethods.includes('BLE')" v-tooltip="$t('discovered_via_bluetooth')"
              class="discovery-icon" />
            <i-lucide:wifi
v-if="d.discoveryMethods.includes('LAN')" v-tooltip="$t('discovered_via_lan')"
              class="discovery-icon" />
            <span v-if="d.status === 'PAIRING'" v-tooltip="$t('waiting_for_confirmation')" class="status-badge warn">{{
              $t('pending') }}</span>
          </template>
          <template #start>
            <v-dropdown v-model="infoOpen[d.id]">
              <template #trigger>
                <DeviceTypeIcon :device-type="d.deviceType" />
              </template>
              <pre class="view-raw">{{ d }}</pre>
            </v-dropdown>
          </template>
          <template #end>
            <v-outlined-button
v-if="d.status === 'PAIRING'" class="btn-sm danger"
              :loading="deviceStates.get(d.id) === DeviceState.CANCELING" @click.stop="cancel(d)">
              {{ $t('cancel') }}
            </v-outlined-button>
            <v-outlined-button
v-else-if="d.status === 'UNPAIRING' || d.status === PeerStatus.PAIRED" class="btn-sm danger"
              :loading="deviceStates.get(d.id) === DeviceState.UNPAIRING" @click.stop="unpair(d)">
              {{ $t('unpair') }}
            </v-outlined-button>
            <v-outlined-button
v-else class="btn-sm" :loading="deviceStates.get(d.id) === DeviceState.PAIRING"
              @click.stop="startPair(d)">
              {{ $t('pair') }}
            </v-outlined-button>
          </template>
        </VListItem>
      </ul>

      <QrPairPanel
        v-model:open="qrOpen"
        collapsible
        @paired="onQrPaired"
      />
    </template>
    <template #actions>
      <v-outlined-button class="btn-sm" @click="handleClose">{{ $t('close') }}</v-outlined-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { popModal } from '@/components/modal'
import MdnsFirewallFix from '@/components/MdnsFirewallFix.vue'
import QrPairPanel from '@/components/QrPairPanel.vue'
import { useDeviceDiscovery, DiscoveryStatus, upsertDiscoveredDevice, type DiscoveredDevice } from '@/hooks/use-device-discovery'
import { useDevicePairing, DeviceState } from '@/hooks/use-device-pairing'
import { unpairPeerGQL, initMutation } from '@/lib/api/mutation'
import type { QrPairedDevice } from '@/lib/device/qr-pairing'
import { useChatStore } from '@/stores/chat'
import { PeerStatus } from '@/lib/status'

const {
  devices: discoveredDevices,
  status,
  retry,
  start,
  stop,
} = useDeviceDiscovery()
const {
  deviceStates,
  pairDevice,
  cancelPairing,
} = useDevicePairing()

const infoOpen = ref<Record<string, boolean>>({})
// empty state is the reason this modal exists — show the fix without an extra click
const qrOpen = ref(false)

const chatStore = useChatStore()

function onQrPaired(d: QrPairedDevice) {
  upsertDiscoveredDevice({
    id: d.id,
    name: d.name,
    ips: d.ip ? [d.ip] : [],
    port: d.port,
    deviceType: d.deviceType,
    version: '',
    platform: '',
    lastSeen: new Date().toISOString(),
    status: PeerStatus.PAIRED,
    discoveryMethods: ['QR'],
  })
  void chatStore.fetchPeers()
}


async function startPair(d: DiscoveredDevice) {
  deviceStates.set(d.id, DeviceState.PAIRING)
  await pairDevice({
    id: d.id,
    name: d.name,
    ips: d.ips,
    port: d.port,
    deviceType: d.deviceType,
    version: d.version,
    platform: d.platform,
    lastSeen: d.lastSeen,
    discoveryMethods: d.discoveryMethods,
  })
  d.status = 'PAIRING'
  deviceStates.delete(d.id)
}

async function cancel(d: DiscoveredDevice) {
  deviceStates.set(d.id, DeviceState.CANCELING)
  await cancelPairing(d.id)
  d.status = PeerStatus.UNPAIRED
  deviceStates.delete(d.id)
}

const { mutate: unpairPeerMute } = initMutation({ document: unpairPeerGQL })

async function unpair(d: DiscoveredDevice) {
  deviceStates.set(d.id, DeviceState.UNPAIRING)
  const result = await unpairPeerMute({ id: d.id })
  if (result) {
    d.status = PeerStatus.UNPAIRED
    deviceStates.set(d.id, DeviceState.UNPAIRED)
    await chatStore.fetchPeers()
  }
  deviceStates.delete(d.id)
}

function handleClose() {
  stop()
  popModal()
}

onMounted(() => {
  start()
  chatStore.fetchPeers()
})

onBeforeUnmount(() => {
  stop()
})
</script>

<style lang="scss" scoped>
.nearby-empty {
  padding-inline: 16px;
  text-align: center;
  color: var(--md-sys-color-on-surface-variant);
}

.qr-pair {
  margin-top: 16px;
}

.discovery-icon {
  width: 16px;
  height: 16px;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
}
</style>
