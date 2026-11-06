<template>
  <v-icon-button v-tooltip="$t('ai.image_search')" @click="modalOpen = true">
    <i-lucide:brain />
    <span v-if="status?.status === ImageSearchStatusType.READY" class="ai-dot" />
  </v-icon-button>
  <AIImageSearchModal v-if="modalOpen" @close="modalOpen = false" />
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useImageSearchStatus } from '@/hooks/ai/use-image-search-status'
import { ImageSearchStatusType } from '@/lib/status'
import AIImageSearchModal from './AIImageSearchModal.vue'

const modalOpen = ref(false)
const { status } = useImageSearchStatus()
</script>

<style lang="scss" scoped>
.ai-dot {
  position: absolute;
  top: 4px;
  right: 4px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--md-sys-color-primary);
}
</style>
