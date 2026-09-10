<template>
  <div v-if="isTauri" class="qr-pair">
    <div v-if="!open" class="qr-entry">
      <v-text-button @click="$emit('update:open', true)">
        {{ $t('device_discovery.qr_pair_entry') }}
      </v-text-button>
    </div>
    <section v-else class="qr-panel">
      <div
        v-if="collapsible"
        class="qr-head qr-head--toggle"
        role="button"
        :aria-expanded="open"
        @click="$emit('update:open', false)"
      >
        <i-material-symbols:qr-code-2-rounded class="qr-head-icon" />
        <span>{{ $t('device_discovery.qr_pair_title') }}</span>
        <div class="grow"></div>
        <i-material-symbols:keyboard-arrow-up-rounded />
      </div>
      <div v-else class="qr-head">
        <i-material-symbols:qr-code-2-rounded class="qr-head-icon" />
        <span>{{ $t('device_discovery.qr_pair_title') }}</span>
      </div>
      <div class="qr-main">
        <div class="qr-code">
          <div v-if="code" class="qr-svg" v-html="code.svg"></div>
          <v-circular-progress v-else indeterminate class="sm" aria-label="loading" />
          <div v-if="phase !== 'idle'" class="qr-overlay">
            <v-circular-progress v-if="phase === 'waiting'" indeterminate class="sm" aria-label="waiting" />
            <i-material-symbols:check-circle-rounded v-else class="qr-ok-icon" />
          </div>
        </div>
        <ol class="qr-steps">
          <li>{{ $t('device_discovery.qr_pair_step1') }}</li>
          <li>{{ $t('device_discovery.qr_pair_step2') }}</li>
          <li>{{ $t('device_discovery.qr_pair_step3') }}</li>
        </ol>
      </div>
      <p v-if="phase === 'waiting'" class="qr-state qr-state--wait">
        <v-circular-progress indeterminate class="sm" aria-label="pairing" />
        <span>{{ $t('device_discovery.qr_pair_waiting', { name: paired?.name }) }}</span>
      </p>
      <p v-else-if="phase === 'success'" class="qr-state qr-state--ok">
        <i-material-symbols:check-circle-rounded />
        <span>{{
          autoLogin && paired
            ? $t('device_discovery.qr_pair_opening_login', { name: paired.name })
            : $t('device_discovery.qr_pair_success')
        }}</span>
      </p>
    </section>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import emitter from '@/plugins/eventbus'
import {
  loadQrPairingCode,
  qrPairedDevice,
  type QrPairingCode,
  type QrPairedDevice,
} from '@/lib/device/qr-pairing'
import type { PairingRequest, PairingResult } from '@/lib/pairing-types'

defineProps({
  open: { type: Boolean, default: false },
  collapsible: { type: Boolean, default: false },
  autoLogin: { type: Boolean, default: false },
})

// Web builds have no local HTTPS server to scan into — no entry, no panel.
const isTauri = __IS_TAURI__

const emit = defineEmits<{
  (e: 'update:open', value: boolean): void
  (e: 'paired', device: QrPairedDevice): void
}>()

const code = ref<QrPairingCode | null>(null)
const phase = ref<'idle' | 'waiting' | 'success'>('idle')
const paired = ref<QrPairedDevice | null>(null)

async function reload() {
  code.value = await loadQrPairingCode()
}

function onRequest(request: PairingRequest) {
  if (phase.value === 'success') return
  paired.value = qrPairedDevice(request)
  phase.value = 'waiting'
}

function onResult(result: PairingResult, ok: boolean) {
  if (phase.value !== 'waiting' || result.deviceId !== paired.value?.id) return
  if (ok) {
    phase.value = 'success'
    emit('paired', paired.value)
  } else {
    phase.value = 'idle'
  }
}

const onSuccess = (r: PairingResult) => onResult(r, true)
const onFailed = (r: PairingResult) => onResult(r, false)
const onCanceled = (r: PairingResult) => onResult(r, false)

onMounted(() => {
  void reload()
  emitter.on('pairing_request_received', onRequest)
  emitter.on('pairing_success', onSuccess)
  emitter.on('pairing_failed', onFailed)
  emitter.on('pairing_canceled', onCanceled)
})

onBeforeUnmount(() => {
  emitter.off('pairing_request_received', onRequest)
  emitter.off('pairing_success', onSuccess)
  emitter.off('pairing_failed', onFailed)
  emitter.off('pairing_canceled', onCanceled)
})
</script>

<style lang="scss" scoped>
.qr-entry {
  text-align: center;
}

.qr-panel {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 12px;
  background: var(--md-sys-color-surface-container-low);
}

.qr-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 500;

  .qr-head-icon {
    width: 18px;
    height: 18px;
    color: var(--md-sys-color-primary);
  }

  .grow {
    flex: 1;
  }
}

.qr-head--toggle {
  width: 100%;
  cursor: pointer;
  padding: 4px 8px;
  margin: -4px -8px;
  border-radius: 8px;

  &:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 4%, transparent);
  }
}

.qr-main {
  display: flex;
  gap: 16px;
  align-items: flex-start;
}

.qr-code {
  position: relative;
  width: 200px;
  height: 200px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 12px;
  border-radius: 12px;
  background: #ffffff;
}

.qr-svg,
.qr-svg :deep(svg) {
  width: 100%;
  height: 100%;
}

.qr-overlay {
  position: absolute;
  inset: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.85);

  .qr-ok-icon {
    width: 48px;
    height: 48px;
    color: var(--md-sys-color-primary);
  }
}

.qr-steps {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin: 0;
  padding: 0 0 0 4px;
  list-style: none;
  counter-reset: qr-step;
  min-width: 0;

  li {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    font-size: 0.85rem;
    line-height: 1.4;
    counter-increment: qr-step;

    &::before {
      content: counter(qr-step);
      flex-shrink: 0;
      width: 20px;
      height: 20px;
      border-radius: 999px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      background: var(--md-sys-color-secondary-container);
      color: var(--md-sys-color-on-secondary-container);
      font-size: 0.72rem;
      font-weight: 700;
    }
  }
}

.qr-state {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  padding: 8px 12px;
  border-radius: 8px;
  font-size: 0.85rem;

  &.qr-state--wait {
    background: var(--md-sys-color-tertiary-container);
    color: var(--md-sys-color-on-tertiary-container);
  }

  &.qr-state--ok {
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
}
</style>
