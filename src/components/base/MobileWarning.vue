<template>
  <MobileWarningDark v-if="isDark" />
  <MobileWarningLight v-else />
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import MobileWarningLight from '@/assets/mobile-warning.svg?component'
import MobileWarningDark from '@/assets/mobile-warning-dark.svg?component'
import emitter from '@/plugins/eventbus'

const isDark = ref(document.documentElement.classList.contains('dark'))

function onColorModeChanged() {
  isDark.value = document.documentElement.classList.contains('dark')
}

onMounted(() => emitter.on('color_mode_changed', onColorModeChanged))
onUnmounted(() => emitter.off('color_mode_changed', onColorModeChanged))
</script>
