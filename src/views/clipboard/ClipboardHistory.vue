<template>
  <section class="clipboard-history">
    <clipboard-item v-for="item in items" :key="item.id" :item="item" @delete="$emit('delete', $event)" />
  </section>
</template>

<script setup lang="ts">
import ClipboardItem from '@/components/ClipboardItem.vue'
import type { IClipboardItem } from '@/lib/interfaces'

defineProps<{ items: IClipboardItem[] }>()
defineEmits<{ delete: [item: IClipboardItem] }>()
</script>

<style lang="scss" scoped>
.clipboard-history {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 16px;
}
</style>
