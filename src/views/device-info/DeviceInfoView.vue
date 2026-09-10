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
          <h5 class="card-title">{{ $t('http_server') }}</h5>
          <div class="card-body">
            <div class="key-value">
              <div class="key">{{ $t('status') }}</div>
              <div class="value">
                <span class="running-dot"></span>{{ $t('running') }}
              </div>
            </div>
            <div class="key-value">
              <div class="key">{{ $t('http_port') }}</div>
              <div class="value">
                {{ httpPort }}
                <v-icon-button @click="openPortDialog('http')">
                  <i-material-symbols:edit-rounded />
                </v-icon-button>
              </div>
            </div>
            <div class="key-value">
              <div class="key">{{ $t('https_port') }}</div>
              <div class="value">
                {{ httpsPort }}
                <v-icon-button @click="openPortDialog('https')">
                  <i-material-symbols:edit-rounded />
                </v-icon-button>
              </div>
            </div>
            <div class="key-value">
              <div class="key">{{ $t('mdns_hostname') }}</div>
              <div class="value">
                {{ mdnsHostname }}
                <v-icon-button @click="openMdnsDialog">
                  <i-material-symbols:edit-rounded />
                </v-icon-button>
                <v-icon-button v-tooltip="$t('mdns_debug')" @click="openMdnsDebug">
                  <i-material-symbols:bug-report-rounded />
                </v-icon-button>
              </div>
            </div>
            <div v-if="httpAddresses.length" class="key-value">
              <div class="key">{{ $t('http_addresses') }}</div>
              <div class="value">
                <div v-for="addr in httpAddresses" :key="addr" class="address-row">{{ addr }}</div>
              </div>
            </div>
            <div v-if="httpsAddresses.length" class="key-value">
              <div class="key">{{ $t('https_addresses') }}</div>
              <div class="value">
                <div v-for="addr in httpsAddresses" :key="addr" class="address-row">{{ addr }}</div>
              </div>
            </div>
          </div>
        </section>
      </div>
      <div>
        <section class="card">
          <h5 class="card-title">{{ $t('device') }}</h5>
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
      <div>
        <section class="card">
          <h5 class="card-title">{{ $t('system') }}</h5>
          <div class="card-body">
            <div v-for="(item, index) in systemInfos" :key="index" class="key-value">
              <div class="key">{{ $t(item.label) }}</div>
              <div class="value">{{ item.value }}</div>
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
      <div v-if="batteryInfos.length">
        <section class="card">
          <h5 class="card-title">{{ $t('battery') }}</h5>
          <div class="card-body">
            <div v-for="(item, index) in batteryInfos" :key="index" class="key-value">
              <div class="key">{{ $t(item.label) }}</div>
              <div class="value">{{ item.value }}</div>
            </div>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onActivated, onDeactivated, computed } from 'vue'
import { formatDateTime, formatDateTimeFull } from '@/lib/format'
import { openModal } from '@/components/modal'
import { useDeviceInfo } from './device-info'
import { useHttpServer } from './use-http-server'
import { useMdns } from './use-mdns'
import { useDlna } from './use-dlna'
import { isLocalMode } from '@/lib/device/client-id'
import MdnsDebugModal from './MdnsDebugModal.vue'
import MdnsHostnameDialog from './MdnsHostnameDialog.vue'
import PortEditDialog from './PortEditDialog.vue'

const isActive = ref(false)
onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })
const localMode = isLocalMode()

const { basicInfos, systemInfos, hardwareInfos, platformInfos, batteryInfos, loading, refetch } = useDeviceInfo()

const {
  httpPort, httpsPort, httpAddresses, httpsAddresses,
  httpPortOptions, httpsPortOptions, loadIps, savePort,
} = useHttpServer()

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

.address-row {
  font-family: monospace;
  font-size: 0.875rem;
  word-break: break-all;
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
