<template>
  <section class="card remote-access-card">
    <header class="remote-access-header">
      <div>
        <h2>{{ $t('remote_access') }}</h2>
        <p>{{ $t('remote_access_description') }}</p>
      </div>
      <v-switch :label="$t('remote_access')" :checked="enabled" :disabled="saving" @change="save" />
    </header>
    <RemoteAccessAddresses
      :http-port="httpPort"
      :https-port="httpsPort"
      :http-addresses="httpAddresses"
      :https-addresses="httpsAddresses"
      :enabled="enabled"
      :loading-ips="loadingIps"
      :address-error="addressError"
      @edit-port="editPort"
      @retry="loadIps"
    />
    <div class="remote-access-hostname">
      <div class="hostname-value">
        <div>
          <span class="hostname-label">{{ $t('mdns_hostname') }}</span
          ><span>{{ hostname || '—' }}</span>
        </div>
        <v-icon-button v-tooltip="$t('edit')" :aria-label="$t('edit') + ': ' + $t('mdns_hostname')" @click="editHostname"><i-material-symbols:edit-rounded /></v-icon-button>
      </div>
      <v-text-button @click="showDiagnostics"
        ><template #icon><i-material-symbols:bug-report-rounded /></template>{{ $t('mdns_debug') }}</v-text-button
      >
    </div>
    <footer class="remote-access-footer" :class="{ online: count !== null && count > 0 }" aria-live="polite">
      <span v-if="count !== null" class="online-dot" aria-hidden="true"></span>
      <span>{{ clientsFailed ? $t('remote_access_clients_unavailable') : count === null ? $t('loading') : $t('remote_access_clients_online', { count }) }}</span>
    </footer>
  </section>
</template>

<script setup lang="ts">
import { openModal } from '@/components/modal'
import { useRemoteAccess } from './use-remote-access'
import { useHttpServer } from './use-http-server'
import { useOnlineClients } from './use-online-clients'
import { useMdns } from './use-mdns'
import RemoteAccessAddresses from './RemoteAccessAddresses.vue'
import MdnsDebugModal from './MdnsDebugModal.vue'
import MdnsHostnameDialog from './MdnsHostnameDialog.vue'
import PortEditDialog from './PortEditDialog.vue'

const { enabled, hostname, saving, save } = useRemoteAccess()
const { httpPort, httpsPort, httpAddresses, httpsAddresses, httpPortOptions, httpsPortOptions, savePort, loadingIps, addressError, loadIps } = useHttpServer()
const { count, failed: clientsFailed } = useOnlineClients()
const { saveHostname } = useMdns()

function editPort(kind: 'http' | 'https') {
  void openModal(PortEditDialog, {
    current: kind === 'http' ? httpPort.value : httpsPort.value,
    options: kind === 'http' ? httpPortOptions.value : httpsPortOptions.value,
    onSave: (port: number) => savePort(kind, port),
  })
}
function editHostname() {
  void openModal(MdnsHostnameDialog, {
    hostname: hostname.value,
    onSave: saveHostname,
  })
}
function showDiagnostics() {
  void openModal(MdnsDebugModal)
}
</script>

<style scoped lang="scss">
.remote-access-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 16px;
  margin-bottom: 24px;
}
.remote-access-header h2 {
  margin: 0 0 4px;
  font-size: 1.25rem;
  font-weight: 600;
}
.remote-access-header p {
  margin: 0;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.875rem;
}
.remote-access-header :deep(.feat-switch) {
  margin-top: 4px;
}
.remote-access-hostname {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 8px 16px;
  margin-top: 16px;
}
.hostname-value {
  display: flex;
  align-items: center;
  min-width: 0;
  gap: 8px;
}
.hostname-value > div {
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow-wrap: anywhere;
}
.hostname-value .btn-icon {
  flex-shrink: 0;
}
.hostname-label {
  font-size: 0.75rem;
  color: var(--md-sys-color-on-surface-variant);
}
.remote-access-footer {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 16px;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.875rem;
}
.remote-access-footer.online {
  color: #247444;
}
:root.dark .remote-access-footer.online {
  color: #88d7a6;
}
.online-dot {
  width: 8px;
  height: 8px;
  flex-shrink: 0;
  border-radius: 50%;
  background: currentColor;
}
</style>
