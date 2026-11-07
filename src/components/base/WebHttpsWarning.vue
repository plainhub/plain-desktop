<template>
  <WebHttpsWarningDark v-if="isDark" />
  <WebHttpsWarningLight v-else />
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import WebHttpsWarningLight from '@/assets/web-https-warning.svg?component'
import WebHttpsWarningDark from '@/assets/web-https-warning-dark.svg?component'
import emitter from '@/plugins/eventbus'

const isDark = ref(document.documentElement.classList.contains('dark'))

function onColorModeChanged() {
  isDark.value = document.documentElement.classList.contains('dark')
}

onMounted(() => emitter.on('color_mode_changed', onColorModeChanged))
onUnmounted(() => emitter.off('color_mode_changed', onColorModeChanged))
</script>
