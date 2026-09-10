<template>
  <div class="scan-status">
    <v-circular-progress indeterminate class="sm" />
    <span>{{ $t('device_discovery.searching') }}</span>
  </div>
  <div v-if="status === DiscoveryStatus.PERMISSION_DENIED" class="status-tip status-tip--error">
    <p class="status-title">{{ $t('device_discovery.lan_permission_denied_title') }}</p>
    <p class="status-body">{{ $t('device_discovery.lan_permission_denied_hint') }}</p>
    <div class="status-actions">
      <v-outlined-button @click="$emit('open-settings')">
        {{ $t('device_discovery.open_system_settings') }}
      </v-outlined-button>
      <v-filled-button @click="$emit('retry')">
        {{ $t('retry') }}
      </v-filled-button>
    </div>
  </div>
  <div v-else-if="status === DiscoveryStatus.NETWORK_ERROR" class="status-tip">
    <p class="status-title">{{ $t('device_discovery.search_failed_title') }}</p>
    <p class="status-body">{{ $t('device_discovery.search_failed_hint') }}</p>
    <div class="status-actions">
      <v-filled-button @click="$emit('retry')">
        {{ $t('retry') }}
      </v-filled-button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { DiscoveryStatus } from '@/hooks/use-device-discovery'

defineProps<{
  status: DiscoveryStatus
}>()

defineEmits<{
  (e: 'retry'): void
  (e: 'open-settings'): void
}>()
</script>

<style lang="scss" scoped>
.scan-status {
  display: flex;
  align-items: center;
  gap: 10px;
  justify-content: center;
  padding: 0 0 16px 0;
  font-size: 0.85rem;
  color: var(--md-sys-color-on-surface-variant);
}

.status-tip {
  padding: 10px 12px;
  margin-bottom: 8px;
  border-radius: 12px;
  background: var(--md-sys-color-surface-container-low);
}

.status-tip--error {
  background: color-mix(in srgb, var(--md-sys-color-error) 8%, var(--md-sys-color-surface));
}

.status-title {
  margin: 0;
  font-size: 0.82rem;
  font-weight: 600;
  color: var(--md-sys-color-on-surface);
}

.status-body {
  margin: 6px 0 0;
  font-size: 0.78rem;
  color: var(--md-sys-color-on-surface-variant);
}

.status-actions {
  display: flex;
  gap: 8px;
  margin-top: 10px;
}
</style>
