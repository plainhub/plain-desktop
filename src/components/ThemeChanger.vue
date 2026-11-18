<template>
  <div class="theme-changer">
    <div class="button-group">
      <button 
        :class="{ 'selected': selectedColorMode === 'ON' }"
        @click="selectColorMode('ON')">
        <i-material-symbols:dark-mode-outline-rounded />
      </button>
      <button 
        :class="{ 'selected': selectedColorMode === 'USE_DEVICE_THEME' }"
        @click="selectColorMode('USE_DEVICE_THEME')">
        <i-material-symbols:brightness-6-outline-rounded />
      </button>
      <button 
        :class="{ 'selected': selectedColorMode === 'OFF' }"
        @click="selectColorMode('OFF')">
        <i-material-symbols:sunny-outline-rounded />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { changeColorMode, getCurrentMode, type ColorMode } from '@/lib/theme'
import { ref } from 'vue'

const selectedColorMode = ref(getCurrentMode())

function selectColorMode(value: ColorMode) {
  selectedColorMode.value = value
  changeColorMode(value)
}
</script>

<style lang="scss" scoped>
.theme-changer {
  margin: 12px 16px;
}
</style>
