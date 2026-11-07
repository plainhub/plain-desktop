<template>
  <VListItem :subtitle="ip">
    <template #start>
      <v-dropdown v-model="infoOpen">
        <template #trigger><PeerIcon :device-type="identity.deviceType" :online="peer?.online" :full-control="fullControl" /></template>
        <pre class="view-raw">{{ peer ?? device }}</pre>
      </v-dropdown>
    </template>
    <template #title>
      <span>{{ identity.name }}</span>
      <span v-if="current" class="status-badge on">{{ $t('device_discovery.current') }}</span>
      <span v-if="pairing" class="status-badge warn">{{ $t('waiting_for_confirmation') }}</span>
    </template>
    <template #end>
      <v-outlined-button v-if="pairing" @click="emit('cancel')">{{ $t('cancel') }}</v-outlined-button>
      <v-outlined-button
        v-else-if="!current"
        :disabled="disabled || (action === 'control' && (!peer?.online || device?.remoteAccess !== true)) || (action === 'switch' && (!peer?.online || device?.remoteAccess === false))"
        @click="emit('connect', action)"
        >{{ $t(actionLabel) }}</v-outlined-button
      >
    </template>
    <template v-if="peer" #actions>
      <v-icon-button :id="menuId" :aria-label="$t('actions')" @click.stop="menuOpen = true"><i-material-symbols:more-vert /></v-icon-button>
      <v-dropdown-menu v-model="menuOpen" :anchor="menuId">
        <div class="dropdown-item" @click="remove">{{ $t('unpair') }}</div>
      </v-dropdown-menu>
    </template>
  </VListItem>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { DiscoveredDevice } from '@/hooks/use-device-discovery'
import type { IPeer } from '@/lib/interfaces'
import { nearbyPairAction } from '@/lib/device/nearby'

const props = defineProps<
  ({ peer: IPeer; device?: DiscoveredDevice } | { device: DiscoveredDevice; peer?: undefined }) & {
    fullControl?: boolean
    current?: boolean
    pairing?: boolean
    disabled?: boolean
  }
>()
const emit = defineEmits<{ connect: [action: ReturnType<typeof nearbyPairAction>]; cancel: []; remove: [] }>()
const identity = computed(() => (props.peer ?? props.device)!)
const ip = computed(() => props.device?.bestIp || props.device?.ips.join(', ') || props.peer?.ip || '')
const action = computed(() => nearbyPairAction(props.peer, props.device, props.fullControl))
const actionLabel = computed(() => ({ switch: 'switch', control: 'pairing_full_control', pair: 'pair', chat: 'pairing_chat_only' })[action.value])
const menuId = computed(() => `nearby-actions-${identity.value.id}`)
const infoOpen = ref(false)
const menuOpen = ref(false)
function remove() {
  menuOpen.value = false
  emit('remove')
}
</script>
