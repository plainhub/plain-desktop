<template>
  <span class="peer-icon" aria-hidden="true">
    <DeviceTypeIcon :device-type="deviceType" />
    <span v-if="fullControl" v-tooltip="$t('login_device_hint')" class="login-badge"><i-material-symbols:key-rounded /></span>
    <span v-if="online" class="online-dot" />
  </span>
</template>

<script setup lang="ts">
import type { DeviceType } from '@/lib/status'

defineProps<{ deviceType?: DeviceType; online?: boolean; fullControl?: boolean }>()
</script>

<style scoped lang="scss">
.peer-icon {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
}
.online-dot {
  position: absolute;
  right: 0;
  bottom: 0;
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: #4caf50;
}
.login-badge {
  position: absolute;
  top: -4px;
  right: -4px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 12px;
  height: 12px;
  border-radius: 999px;
  background: var(--md-sys-color-primary);
  color: var(--md-sys-color-on-primary);
  box-shadow: 0 0 0 2px var(--md-sys-color-surface);

  svg {
    width: 8px;
    height: 8px;
  }
}
</style>
