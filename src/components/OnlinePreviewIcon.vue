<template>
  <span
    v-if="isPreviewable"
    v-tooltip="$t('supports_online_preview')"
    class="online-preview-icon"
  >⚡</span>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { canOpenInBrowser, canOpenInTextViewer } from '@/lib/file'

const props = withDefaults(
  defineProps<{
    name: string
    isDir?: boolean
  }>(),
  { isDir: false },
)

const isPreviewable = computed(() => !props.isDir && (canOpenInTextViewer(props.name) || canOpenInBrowser(props.name)))
</script>
