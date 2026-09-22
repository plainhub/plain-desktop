<template>
  <v-icon-button v-if="hasFeature(Capability.IMAGE_SEARCH, app.capabilities) && app.buildChannel !== AppChannelType.FDROID" v-tooltip="$t('ai.image_search')" @click="modalOpen = true">
    <i-lucide:brain />
    <span v-if="status?.status === ImageSearchStatusType.READY" class="ai-dot" />
  </v-icon-button>
  <AIImageSearchModal v-if="modalOpen" @close="modalOpen = false" />
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useImageSearchStatus } from '@/hooks/ai/use-image-search-status'
import { ImageSearchStatusType, AppChannelType } from '@/lib/status'
import { Capability } from '@/lib/data'
import { hasFeature } from '@/lib/feature'
import AIImageSearchModal from './AIImageSearchModal.vue'
import { useTempStore } from '@/stores/temp'

const { app } = storeToRefs(useTempStore())
const modalOpen = ref(false)
const { status } = useImageSearchStatus()
</script>

<style lang="scss" scoped>
.ai-dot {
  position: absolute; top: 6px; right: 6px; width: 6px; height: 6px;
  border-radius: 50%; background: var(--md-sys-color-primary);
}
</style>
