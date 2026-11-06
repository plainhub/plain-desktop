<template>
  <v-modal width="520px" @close="close">
    <template #headline>{{ $t('mdns_debug') }}</template>
    <template #content>
      <div class="mdns-activity-heading">{{ $t('mdns_activity') }}</div>
      <div v-if="!activity.length" class="mdns-empty">{{ $t('mdns_no_activity') }}</div>
      <div v-for="(entry, index) in activity" :key="`${entry.time}-${index}`" class="mdns-activity">
        <div class="mdns-activity-title">{{ new Date(entry.time).toLocaleTimeString() }} · {{ $t(`mdns_${entry.event}`) }}</div>
        <div class="mdns-activity-detail">{{ entry.detail }}</div>
      </div>
      <div class="mdns-activity-heading">{{ $t('mdns_service_type') }}</div>
      <div v-if="!snapshots.length" class="mdns-empty">{{ $t('mdns_no_devices') }}</div>
      <div v-for="s in snapshots" :key="s.instanceFqdn" class="mdns-device">
        <div class="mdns-fqdn" :class="{ incomplete: !s.complete }">{{ s.instanceFqdn }}</div>
        <div class="key-value">
          <div class="key">{{ $t('mdns_service_type') }}</div>
          <div class="value">{{ s.serviceType || $t('not_available') }}</div>
        </div>
        <div class="key-value">
          <div class="key">{{ $t('mdns_instance') }}</div>
          <div class="value">{{ s.instanceName || $t('not_available') }}</div>
        </div>
        <div class="key-value">
          <div class="key">{{ $t('mdns_hostname') }}</div>
          <div class="value">{{ s.hostname || $t('not_available') }}</div>
        </div>
        <div class="key-value">
          <div class="key">{{ $t('port') }}</div>
          <div class="value">{{ s.port > 0 ? s.port : $t('not_available') }}</div>
        </div>
        <div class="key-value">
          <div class="key">{{ $t('mdns_ips') }}</div>
          <div class="value">{{ [...s.ips, ...s.ipv6].join(', ') || $t('not_available') }}</div>
        </div>
        <div class="key-value">
          <div class="key">{{ $t('mdns_txt') }}</div>
          <div class="value mdns-txt">{{ s.txtRecords.join('\n') || $t('not_available') }}</div>
        </div>
      </div>
    </template>
    <template #actions>
      <v-outlined-button @click="paused = !paused">{{ $t(paused ? 'resume' : 'pause') }}</v-outlined-button>
      <v-outlined-button @click="refresh">{{ $t('refresh') }}</v-outlined-button>
      <v-filled-button @click="close">{{ $t('close') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted } from 'vue'
import { popModal } from '@/components/modal'
import { useMdns } from './use-mdns'

// The modal owns its browsing lifecycle (mirrors plain-app's MdnsDebugPage):
// while open it keeps periodic discovery running and refreshes the snapshot
// every two seconds; it stops discovery on close only when it started it.
const { snapshots, activity, paused, refreshSnapshot, startBrowsing, stopBrowsing } = useMdns()

function refresh() {
  refreshSnapshot()
}

function close() {
  popModal()
}

onMounted(() => {
  startBrowsing()
})

onBeforeUnmount(() => {
  stopBrowsing()
})
</script>

<style lang="scss" scoped>
.mdns-empty {
  color: var(--md-sys-color-on-surface-variant, #666);
  padding: 24px 0;
  text-align: center;
}

.mdns-activity-heading {
  font-weight: 600;
  margin: 16px 0 8px;
}

.mdns-activity {
  border-bottom: 1px solid var(--md-sys-color-outline-variant, #ccc);
  padding: 8px 0;
}

.mdns-activity-title {
  font-weight: 600;
}

.mdns-activity-detail {
  color: var(--md-sys-color-on-surface-variant, #666);
  overflow-wrap: anywhere;
}

.mdns-device {
  border: 1px solid var(--md-sys-color-outline-variant, #ccc);
  border-radius: 8px;
  padding: 8px 12px;
  margin-bottom: 12px;
}

.mdns-fqdn {
  font-weight: 600;
  margin-bottom: 4px;
  word-break: break-all;
}

.mdns-fqdn.incomplete {
  color: var(--md-sys-color-error, #ba1a1a);
}

.mdns-txt {
  font-family: monospace;
  font-size: 0.875rem;
  white-space: pre-line;
  word-break: break-all;
}
</style>
