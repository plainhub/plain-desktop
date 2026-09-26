<template>
  <v-modal width="420px" @close="deny">
    <template #headline>{{ $t('pair_request') }}</template>
    <template #content>
      <div class="pairing-request">
        <div class="pairing-icon">
          <DeviceTypeIcon :device-type="request.deviceType" />
        </div>
        <p class="pairing-message">
          {{ $t('pairing_request_message', { name: request.fromName }) }}
        </p>
        <p class="pairing-access">{{ $t('pairing_chat_only') }}</p>
        <p class="pairing-hint">
          {{ $t('pairing_request_hint') }}
        </p>
        <ul class="card list-items">
          <v-list-item :title="$t('ip_address')" :value="displayIp">
          </v-list-item>
        </ul>
        <div class="pairing-actions">
          <v-filled-button
            class="btn-block"
            :loading="allowBusy"
            :disabled="busy"
            @click="allow"
          >
            {{ $t('allow') }}
          </v-filled-button>
          <v-outlined-button
            class="btn-block danger"
            :loading="denyBusy"
            :disabled="busy"
            @click="deny"
          >
            {{ $t('deny') }}
          </v-outlined-button>
        </div>
      </div>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import type { PropType } from 'vue'
import { popModal } from '@/components/modal'
import emitter from '@/plugins/eventbus'
import type { PairingRequest, PairingResult } from '@/lib/pairing-types'
import { allowPairing, denyPairing } from '@/hooks/use-device-pairing'

const props = defineProps({
  request: { type: Object as PropType<PairingRequest>, required: true },
})

const allowBusy = ref(false)
const denyBusy = ref(false)
const busy = computed(() => allowBusy.value || denyBusy.value)

const displayIp = computed(() => {
  if (props.request.fromIp) return props.request.fromIp
  const ips = props.request.ips ?? []
  return ips[0] ?? ''
})

function dismissOnRemoteResult(result: PairingResult) {
  if (busy.value) return
  if (result.deviceId !== props.request.fromId) return
  popModal()
}

const handleSuccess = (r: PairingResult) => dismissOnRemoteResult(r)
const handleFailed = (r: PairingResult) => dismissOnRemoteResult(r)
const handleCanceled = (r: PairingResult) => dismissOnRemoteResult(r)

emitter.on('pairing_success', handleSuccess)
emitter.on('pairing_failed', handleFailed)
emitter.on('pairing_canceled', handleCanceled)

onBeforeUnmount(() => {
  emitter.off('pairing_success', handleSuccess)
  emitter.off('pairing_failed', handleFailed)
  emitter.off('pairing_canceled', handleCanceled)
})

async function allow() {
  if (busy.value) return
  allowBusy.value = true
  try {
    await allowPairing(props.request)
  } finally {
    popModal()
  }
}

async function deny() {
  if (busy.value) return
  denyBusy.value = true
  try {
    await denyPairing(props.request)
  } finally {
    popModal()
  }
}
</script>

<style lang="scss" scoped>
.pairing-request {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 20px;
  text-align: center;
}

.pairing-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 88px;
  height: 88px;
  border-radius: 50%;
  background-color: var(--md-sys-color-primary-container);
  color: var(--md-sys-color-on-primary-container);

  :deep(svg) {
    width: 44px;
    height: 44px;
    fill: none;
    stroke: currentColor;
  }
}

.pairing-message {
  margin: 0;
  font-size: 1rem;
  line-height: 1.4;
  color: var(--md-sys-color-on-surface);
}

.pairing-access {
  margin: 0;
  padding: 4px 12px;
  border-radius: 999px;
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}

.pairing-hint {
  margin: 0;
  font-size: 0.875rem;
  line-height: 1.4;
  color: var(--md-sys-color-on-surface-variant);
}

.pairing-actions {
  display: flex;
  flex-direction: column;
  gap: 12px;
  width: 100%;
}
</style>
