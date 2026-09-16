<template>
  <div class="peer-group" :class="{ offline: !online }">
    <div class="group-head" @click="collapsed = !collapsed">
      <DeviceTypeIcon :device-type="deviceType" />
      <span class="g-name nowrap">{{ name }}</span>
      <span v-tooltip="$t(online ? 'online' : 'offline')" class="dot" :class="online ? 'on' : 'off'"></span>
      <span class="g-count">{{ count }}</span>
      <slot name="actions"></slot>
      <button
        v-if="clearable"
        v-tooltip="$t('clear_list')"
        class="btn-icon"
        @click.stop="$emit('clear')"
      >
        <i-material-symbols:delete-forever-outline-rounded />
      </button>
      <span class="chev" :class="{ closed: collapsed }">
        <i-material-symbols:keyboard-arrow-down-rounded />
      </span>
    </div>
    <div v-if="!collapsed" class="grp-body">
      <slot></slot>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import DeviceTypeIcon from '@/components/DeviceTypeIcon.vue'
import type { DeviceType } from '@/lib/status'

defineProps<{
  name: string
  deviceType: DeviceType
  online: boolean
  count: number
  clearable: boolean
}>()

defineEmits<{
  clear: []
}>()

const collapsed = ref(false)
</script>

<style lang="scss" scoped>
.peer-group {
  // flex item inside the scrollable .quick-content-body column: overflow:hidden
  // would zero its automatic min-size and let siblings squash instead of overflow
  flex-shrink: 0;
  border-radius: var(--pl-shape-l);
  background-color: var(--md-sys-color-surface-container);
  overflow: hidden;
  margin: 0 16px;

  &.offline {
    .grp-body {
      opacity: 0.56;
    }
  }
}

.group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 8px 8px 12px;
  font-size: 0.85rem;
  cursor: pointer;
  user-select: none;
  min-height: 48px;
  box-sizing: border-box;

  .g-name {
    font-weight: 600;
  }

  .g-count {
    margin-inline-start: auto;
    font-size: 0.72rem;
    color: var(--md-sys-color-on-surface-variant);
    background-color: var(--md-sys-color-surface-container-high);
    border-radius: 999px;
    padding: 2px 8px;
  }

  .btn-icon {
    width: 32px;
    height: 32px;

    svg {
      width: 16px;
      height: 16px;
    }
  }

  .chev {
    display: flex;
    color: var(--md-sys-color-on-surface-variant);
    transition: transform 0.2s ease;

    &.closed {
      transform: rotate(-90deg);
    }
  }
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;

  &.on {
    background-color: var(--md-sys-color-primary);
  }

  &.off {
    background-color: var(--md-sys-color-outline-variant);
  }
}

.grp-body {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 0 8px 8px 8px;
}
</style>
