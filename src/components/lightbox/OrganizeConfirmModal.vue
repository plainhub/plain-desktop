<template>
  <v-modal @close="finish(false)">
    <template #headline>
      {{ $t('organize_summary_title') }}
    </template>
    <template #content>
      <p class="summary-line">{{ $t('organize_summary_body', { count }) }}</p>
    </template>
    <template #actions>
      <v-outlined-button @click="finish(true)">
        <i-material-symbols:undo-rounded />
        {{ $t('organize_undo_all') }}
      </v-outlined-button>
      <v-filled-button @click="finish(false)">
        {{ $t('organize_keep_changes') }}
      </v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { Modal, popModal } from '@/components/modal'

defineProps({
  count: { type: Number, required: true },
})

const emit = defineEmits<{
  (e: typeof Modal.EVENT_PROMPT, undo: boolean): void
}>()

// resolves promptModal<boolean> with true → undo all, false → keep changes
function finish(undo: boolean) {
  emit(Modal.EVENT_PROMPT, undo)
  popModal()
}
</script>

<style lang="scss" scoped>
.summary-line {
  margin: 0;
}
</style>
