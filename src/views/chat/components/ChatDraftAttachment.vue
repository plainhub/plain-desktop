<template>
  <div class="staged-attachment">
    <img v-if="preview" :src="preview" alt="" class="staged-preview" />
    <i-material-symbols:attach-file-rounded v-else aria-hidden="true" />
    <span :title="attachment.file.name">{{ attachment.file.name }}</span>
    <v-icon-button :aria-label="`${$t('remove')}: ${attachment.file.name}`" :disabled="disabled" @click="$emit('remove')">
      <i-material-symbols:close-rounded />
    </v-icon-button>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import type { DraftAttachment } from '@/lib/chat/message-draft'

const props = defineProps<{ attachment: DraftAttachment; disabled: boolean }>()
defineEmits<{ remove: [] }>()
const preview = ref('')
function release() {
  if (preview.value) URL.revokeObjectURL(preview.value)
  preview.value = ''
}
watch(() => props.attachment, ({ type, file }) => {
  release()
  if (type === 'IMAGE' && file.type.startsWith('image/')) preview.value = URL.createObjectURL(file)
}, { immediate: true })
onBeforeUnmount(release)
</script>

<style scoped>
.staged-attachment { display: flex; align-items: center; gap: 4px; max-width: 100%; padding-left: 8px; border-radius: 8px; background: var(--md-sys-color-surface-container); }
.staged-attachment span { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.staged-preview { width: 36px; height: 36px; border-radius: 6px; object-fit: cover; }
</style>
