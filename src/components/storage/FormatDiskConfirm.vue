<template>
  <v-modal @close="popModal">
    <template #headline>
      {{ $t('format_disk') }}
    </template>

    <template #content>
      <div class="body">{{ $t('format_disk_confirm', { disk: diskLabel }) }}</div>
    </template>

    <template #actions>
      <v-outlined-button @click="popModal">{{ $t('cancel') }}</v-outlined-button>
      <v-filled-button :loading="loading" @click="doFormat">{{ $t('format_disk') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { PropType } from 'vue'
import { formatDiskGQL, initMutation } from '@/lib/api/mutation'
import { popModal } from '@/components/modal'

const props = defineProps({
  path: { type: String, required: true },
  label: { type: String, default: '' },
  onDone: {
    type: Function as PropType<() => void>,
    default: () => {},
  },
})

const diskLabel = computed(() => props.label || props.path)

const { mutate, loading, onDone: onFormatDone } = initMutation({
  document: formatDiskGQL,
})

function doFormat() {
  mutate({ path: props.path }).catch(() => {
    // handled by initMutation
  })
}

onFormatDone(() => {
  // Give the backend's auto-mount reconciliation a moment to settle so the
  // refreshed UI reflects the new mounted volume.
  setTimeout(() => {
    props.onDone?.()
  }, 10_000)
  popModal()
})
</script>

<style scoped>
.body {
  color: var(--md-sys-color-on-surface-variant);
  white-space: pre-wrap;
}
</style>
