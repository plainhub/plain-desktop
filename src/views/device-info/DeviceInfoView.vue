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
                <v-switch :label="$t('dlna_receiver')" :checked="dlnaEnabled" :disabled="dlnaLoading" @change="toggleDlna" />
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
import { useDeviceInfo } from './device-info'
import { useDlna } from './use-dlna'

const isActive = ref(false)
onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })
const localMode = isLocalMode()

const { basicInfos, hardwareInfos, platformInfos, statusInfos, loading, refetch } = useDeviceInfo()

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

onActivated(() => {
  loadDlna()
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

@media (max-width: 480px) {
  .grids {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
