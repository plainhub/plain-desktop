<template>
  <peer-group-shell
    :name="group.name"
    :device-type="group.deviceType"
    :online="group.online"
    :count="group.items.length"
    :clearable="group.online && group.items.length > 0"
    @clear="$emit('clear')"
  >
    <slot></slot>
  </peer-group-shell>
</template>

<script setup lang="ts">
import PeerGroupShell from '@/components/PeerGroupShell.vue'
import type { PeerNotificationGroup } from '@/lib/peer/local-peer-data'

defineProps<{
  group: PeerNotificationGroup
}>()

defineEmits<{
  clear: []
}>()
</script>
