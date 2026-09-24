<template>
  <VListItem :subtitle="member.isSelf ? $t('this_device') : member.ip">
    <template #title>
      <span>{{ member.name }}</span>
      <span v-if="member.isOwner" class="status-badge on">{{ $t('creator') }}</span>
      <span v-else-if="member.status === MemberStatus.PENDING" v-tooltip="$t('waiting_for_confirmation')" class="status-badge warn">{{ $t('pending') }}</span>
    </template>
    <template v-if="member.deviceType" #start>
      <v-dropdown v-model="rawOpen">
        <template #trigger>
          <DeviceTypeIcon :device-type="member.deviceType" />
        </template>
        <pre class="view-raw">{{ member }}</pre>
      </v-dropdown>
    </template>
    <template #end>
      <slot name="end" />
    </template>
  </VListItem>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { MemberStatus } from '@/lib/status'

export interface IChannelMemberListItem {
  peerId: string
  name: string
  ip?: string
  deviceType?: string
  isSelf: boolean
  isOwner: boolean
  status: MemberStatus
}

const rawOpen = ref(false)

defineProps<{
  member: IChannelMemberListItem
}>()
</script>
