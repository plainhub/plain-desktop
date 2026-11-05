<template>
  <Teleport v-if="isActive" to="#header-end-slot" defer>
    <v-icon-button v-tooltip="$t('refresh')" :loading="loading" @click="refetch">
      <i-material-symbols:refresh-rounded />
    </v-icon-button>
  </Teleport>
  <div class="scroll-content">
    <div class="grids">
      <div v-if="localMode" style="display: none;">
        <section class="card">
          <h5 class="card-title">{{ $t('dlna_receiver') }}</h5>
          <div class="card-body">
            <div class="key-value">
              <div class="key">{{ $t('dlna_receiver_desc') }}</div>
              <div class="value">
                <label class="feat-switch" :class="{ disabled: dlnaLoading }">
                  <input type="checkbox" :checked="dlnaEnabled" :disabled="dlnaLoading" @change="toggleDlna" />
                  <span class="switch-track"></span>
                </label>
              </div>
            </div>
            <template v-if="dlnaEnabled">
              <div class="key-value">
                <div class="key">{{ $t('status') }}</div>
                <div class="value">
                  <span v-if="dlnaRunning" class="running-dot"></span>{{ dlnaRunning ? $t('running') : $t('stopped') }}
                </div>
              </div>
              <div v-if="dlnaRunning && dlnaPort" class="key-value">
                <div class="key">{{ $t('port') }}</div>
                <div class="value">{{ dlnaPort }}</div>
              </div>
              <div v-if="dlnaStartError" class="key-value">
                <div class="key">{{ $t('error') }}</div>
                <div class="value">
                  {{ dlnaStartError }}
                  <v-icon-button v-tooltip="$t('retry')" :loading="dlnaRetrying" @click="toggleDlna">
                    <i-material-symbols:refresh-rounded />
                  </v-icon-button>
                </div>
              </div>
            </template>
          </div>
        </section>
      </div>
      <div v-if="localMode">
        <section class="card">
          <header class="server-header">
            <h5 class="card-title">{{ $t('http_server') }}</h5>
            <span class="server-status"><span class="running-dot" aria-hidden="true"></span>{{ $t('running') }}</span>
          </header>
          <div class="card-body server-content">
            <div class="server-setting">
              <span>{{ $t('mdns_hostname') }}</span>
              <div class="server-value">
                <span>{{ mdnsHostname }}</span>
                <v-icon-button :aria-label="$t('edit') + ': ' + $t('mdns_hostname')" v-tooltip="$t('edit')" @click="openMdnsDialog">
                  <i-material-symbols:edit-rounded />
                </v-icon-button>
              </div>
            </div>
            <section v-for="group in serverProtocols" :key="group.kind" :aria-label="group.label" class="server-protocol">
              <div class="server-setting">
                <h6>{{ group.label }}</h6>
                <v-text-button type="button" :aria-label="$t('edit_port') + ': ' + group.label + ' ' + group.port" @click="openPortDialog(group.kind)">
                  {{ $t('port') }} {{ group.port }}
                  <i-material-symbols:edit-rounded aria-hidden="true" />
                </v-text-button>
              </div>
              <div v-for="addr in group.addresses" :key="addr" class="server-address">
                <code>{{ addr }}</code>
                <v-copy-button class="server-copy" :text="addr" :aria-label="$t('copy') + ': ' + addr" v-tooltip="$t('copy')" />
              </div>
            </section>
            <div class="server-actions">
              <v-text-button type="button" @click="openMdnsDebug">
                <template #icon><i-material-symbols:bug-report-rounded aria-hidden="true" /></template>
                {{ $t('mdns_debug') }}
              </v-text-button>
            </div>
          </div>
        </section>
      </div>
      <div>
        <section class="card">
          <h5 class="card-title">{{ $t('device_info') }}</h5>
          <div class="card-body">
            <div v-for="(item, index) in basicInfos" :key="index" class="key-value">
              <div class="key">{{ $t(item.label) }}</div>
              <div class="value">
                <template v-if="Array.isArray(item.value)">
                  <div v-for="(it, i) in item.value" :key="i">{{ it }}</div>
                </template>
                <template v-else>{{ item.value }}</template>
              </div>
            </div>
          </div>
        </section>
      </div>
      <div v-if="hardwareInfos.length">
        <section class="card">
          <h5 class="card-title">{{ $t('hardware') }}</h5>
          <div class="card-body">
            <div v-for="(item, index) in hardwareInfos" :key="index" class="key-value">
              <div class="key">{{ $t(item.label) }}</div>
              <div class="value">{{ item.value }}</div>
            </div>
          </div>
        </section>
      </div>
      <div v-if="platformInfos.length">
        <section class="card">
          <h5 class="card-title">{{ $t('platform_info') }}</h5>
          <div class="card-body">
            <div v-for="(item, index) in platformInfos" :key="index" class="key-value">
              <div class="key">{{ $t(item.label) }}</div>
              <div class="value">
                <time v-if="item.isTime" v-tooltip="formatDateTimeFull(item.value)" class="time">{{ formatDateTime(item.value) }}</time>
                <template v-else>{{ item.value }}</template>
              </div>
            </div>
          </div>
        </section>
      </div>
      <div v-if="statusInfos.length">
        <section class="card">
          <h5 class="card-title">{{ $t('device_status') }}</h5>
          <div class="card-body">
            <div v-for="(item, index) in statusInfos" :key="index" class="key-value">
              <div class="key">{{ $t(item.label) }}</div>
              <div class="value">
                <template v-if="Array.isArray(item.value)">
                  <div v-for="(it, i) in item.value" :key="i">{{ it }}</div>
                </template>
                <template v-else>{{ item.value }}</template>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { isLocalMode } from '@/lib/device/local-mode'
import { ref, onActivated, onDeactivated, computed } from 'vue'
import { formatDateTime, formatDateTimeFull } from '@/lib/format'
import { openModal } from '@/components/modal'
import { useDeviceInfo } from './device-info'
import { useHttpServer } from './use-http-server'
import { useMdns } from './use-mdns'
import { useDlna } from './use-dlna'
import MdnsDebugModal from './MdnsDebugModal.vue'
import MdnsHostnameDialog from './MdnsHostnameDialog.vue'
import PortEditDialog from './PortEditDialog.vue'

const isActive = ref(false)
onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })
const localMode = isLocalMode()

const { basicInfos, hardwareInfos, platformInfos, statusInfos, loading, refetch } = useDeviceInfo()

const {
  httpPort, httpsPort, httpAddresses, httpsAddresses,
  httpPortOptions, httpsPortOptions, loadIps, savePort,
} = useHttpServer()

const serverProtocols = computed(() => [
  { kind: 'http' as const, label: 'HTTP', port: httpPort.value, addresses: httpAddresses.value },
  { kind: 'https' as const, label: 'HTTPS', port: httpsPort.value, addresses: httpsAddresses.value },
])

const {
  hostname: mdnsHostname,
  loadHostname: loadMdnsHostname, saveHostname: saveMdnsHostname,
} = useMdns()

const {
  snapshot: dlnaSnapshot, loading: dlnaLoading, load: loadDlna, setEnabled: setDlnaEnabled,
} = useDlna()

const dlnaEnabled = computed(() => dlnaSnapshot.value?.enabled ?? false)
const dlnaRunning = computed(() => dlnaSnapshot.value?.isRunning ?? false)
const dlnaPort = computed(() => dlnaSnapshot.value?.port ?? 0)
const dlnaStartError = computed(() => dlnaSnapshot.value?.startError ?? '')
const dlnaRetrying = computed(() => dlnaSnapshot.value?.isRetrying ?? false)

async function toggleDlna() {
  await setDlnaEnabled(!dlnaEnabled.value)
}

function openMdnsDialog() {
  openModal(MdnsHostnameDialog, {
    hostname: mdnsHostname.value,
    onSave: saveMdnsHostname,
  })
}

function openPortDialog(kind: 'http' | 'https') {
  openModal(PortEditDialog, {
    current: kind === 'http' ? httpPort.value : httpsPort.value,
    options: kind === 'http' ? httpPortOptions.value : httpsPortOptions.value,
    onSave: (port: number) => savePort(kind, port),
  })
}

function openMdnsDebug() {
  openModal(MdnsDebugModal)
}

onActivated(() => {
  loadIps()
  loadDlna()
  if (localMode) loadMdnsHostname()
})
</script>
<style lang="scss" scoped>
.scroll-content {
  padding: 0 0 16px 0;
}

.grids {
  display: grid;
  gap: 16px;
  padding: 16px;
  grid-template-columns: repeat(auto-fit, minmax(350px, 1fr));
}

.card {
  height: 100%;
  margin-bottom: 12px;
}

.running-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: #4caf50;
  margin-right: 6px;
  vertical-align: middle;
}

.server-header,
.server-setting,
.server-address {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px 16px;
}

.server-header,
.server-setting {
  flex-wrap: wrap;
}

.server-status {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 16px;
  font-size: 0.875rem;
}

.server-status .running-dot {
  margin: 0;
}

.server-content {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.server-value {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  min-width: 0;
  margin-inline-start: auto;
}

.server-value > span {
  overflow-wrap: anywhere;
}

.server-value .btn-icon {
  flex-shrink: 0;
}

.server-setting .v-text-button {
  margin-inline-start: auto;
  font-variant-numeric: tabular-nums;
}

.server-protocol h6 {
  margin: 0;
  font-size: 0.875rem;
  font-weight: 600;
}

.server-address {
  padding-block: 8px;
}

.server-address code {
  min-width: 0;
  font-family: monospace;
  font-size: 0.8125rem;
  overflow-wrap: anywhere;
}

.server-copy.v-copy-button {
  flex-shrink: 0;
  width: 32px;
  height: 32px;
}

.server-copy :deep(svg) {
  width: 16px;
  height: 16px;
}

.server-actions {
  display: flex;
  justify-content: flex-end;
}

.server-content button:focus-visible {
  outline: 2px solid var(--md-sys-color-primary);
  outline-offset: 2px;
}

@media (max-width: 480px) {
  .grids {
    grid-template-columns: minmax(0, 1fr);
  }
}

@media (pointer: coarse) {
  .server-copy.v-copy-button,
  .server-value .btn-icon,
  .server-content .v-text-button {
    min-width: 48px;
    min-height: 48px;
  }
}
.feat-switch {
  cursor: pointer;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
}

.feat-switch.disabled {
  cursor: not-allowed;
  opacity: 0.4;
}

.feat-switch input {
  position: absolute;
  opacity: 0;
  width: 0;
  height: 0;
}

.switch-track {
  width: 52px;
  height: 32px;
  border-radius: 16px;
  background: var(--md-sys-color-surface-container-highest);
  position: relative;
  transition: background 0.25s;
}

.switch-track::after {
  content: '';
  position: absolute;
  top: 4px;
  left: 4px;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--md-sys-color-outline);
  transition: transform 0.25s, background 0.25s;
}

input:checked + .switch-track {
  background: var(--md-sys-color-primary);
}

input:checked + .switch-track::after {
  transform: translateX(20px);
  background: var(--md-sys-color-on-primary);
}

</style>
