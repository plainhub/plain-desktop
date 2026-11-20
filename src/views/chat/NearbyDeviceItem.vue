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
      <i-lucide:check v-if="current" role="img" :aria-label="$t('device_discovery.current')" />
      <span v-if="pairing" class="status-badge warn">{{ $t('waiting_for_confirmation') }}</span>
    </template>
    <template #end>
      <v-outlined-button v-if="pairing" @click="emit('cancel')">{{ $t('cancel') }}</v-outlined-button>
      <v-icon-button
        v-else-if="!current && action === 'switch'"
        :disabled="connectDisabled"
        :loading="switching"
        :aria-label="$t(switching ? 'switching_device' : 'switch')"
        :title="$t('switch')"
        @click="emit('connect', action)"
      ><i-lucide:arrow-left-right /></v-icon-button>
      <v-outlined-button v-else-if="!peer && !current" :disabled="disabled" @click="emit('connect', action)">
        {{ $t(actionLabel) }}
      </v-outlined-button>
    </template>
    <template v-if="peer && !pairing" #actions>
      <v-icon-button :id="menuId" :aria-label="$t('actions')" :disabled="disabled" @click.stop="menuOpen = true"><i-material-symbols:more-horiz /></v-icon-button>
      <v-dropdown-menu v-model="menuOpen" :anchor="menuId">
        <button v-if="!current && action === 'control'" type="button" class="dropdown-item" :class="{ disabled: connectDisabled }" :disabled="connectDisabled" @click="menuOpen = false; emit('connect', action)">{{ $t(actionLabel) }}</button>
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
    pending?: boolean
    disabled?: boolean
    switching?: boolean
  }
>()
const emit = defineEmits<{ connect: [action: ReturnType<typeof nearbyPairAction>]; cancel: []; remove: [] }>()
const identity = computed(() => (props.peer ?? props.device)!)
const pairing = computed(() => props.pending || props.device?.status === 'PAIRING')
const ip = computed(() => props.peer ? props.peer.ip : props.device?.bestIp || props.device?.ips.join(', ') || '')
const action = computed(() => nearbyPairAction(props.peer, props.device, props.fullControl))
const connectDisabled = computed(() => props.disabled || (action.value === 'control' && (!props.peer?.online || props.device?.remoteAccess !== true)))
const actionLabel = computed(() => ({ switch: 'switch', control: 'pairing_full_control', pair: 'pair', chat: 'pairing_chat_only' })[action.value])
const menuId = computed(() => `nearby-actions-${identity.value.id}`)
const infoOpen = ref(false)
const menuOpen = ref(false)
function remove() {
  menuOpen.value = false
  emit('remove')
}
</script>

<style scoped>
button.dropdown-item {
  border: 0;
  background: transparent;
  color: inherit;
  width: 100%;
  font-family: inherit;
  text-align: start;
}
</style>
