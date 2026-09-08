<template>
  <div v-if="cardVisible" class="status-tip status-tip--error">
    <p class="status-title">{{ $t('device_discovery.firewall_blocked_title') }}</p>
    <p class="status-body">{{ $t('device_discovery.firewall_blocked_hint') }}</p>
    <p v-if="state === 'fixed'" class="status-body fw-result">
      {{ $t('device_discovery.firewall_fixed') }}
    </p>
    <p v-else-if="state === 'failed'" class="status-body fw-result">
      {{ $t('device_discovery.firewall_fix_failed') }}
    </p>
    <div v-if="state !== 'fixed'" class="status-actions">
      <v-filled-button
        class="sm"
        :loading="state === 'fixing'"
        :disabled="state === 'fixing'"
        @click="onFix"
      >
        {{ $t('device_discovery.firewall_fix') }}
      </v-filled-button>
      <v-outlined-button class="sm" @click="manualOpen = !manualOpen">
        {{ $t('device_discovery.firewall_manual') }}
      </v-outlined-button>
    </div>
    <div v-if="manualOpen || state === 'failed'" class="fw-manual">
      <p class="status-body">{{ $t('device_discovery.firewall_manual_intro') }}</p>
      <pre class="fw-cmd">{{ manualCommands }}</pre>
      <button class="fw-copy" type="button" @click="copyCommands">
        {{ copied ? '✓' : $t('copy') }}
      </button>
    </div>
  </div>
  <div v-else-if="softVisible" class="soft-hint">
    <button class="soft-toggle" type="button" @click="softOpen = !softOpen">
      {{ $t('device_discovery.no_devices_tip') }}
    </button>
    <ul v-if="softOpen" class="soft-list">
      <li>{{ $t('device_discovery.tip_same_network') }}</li>
      <li>{{ $t('device_discovery.tip_app_open') }}</li>
      <li>{{ $t('device_discovery.tip_antivirus') }}</li>
      <li>{{ $t('device_discovery.tip_ap_isolation') }}</li>
    </ul>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useMdnsFirewall } from '@/hooks/use-mdns-firewall'

const props = defineProps<{
  deviceCount: number
}>()

const emit = defineEmits<{
  (e: 'fixed'): void
}>()

const { state, exePath, probe, fix } = useMdnsFirewall()

const manualOpen = ref(false)
const softOpen = ref(false)
const copied = ref(false)
const softArmed = ref(false)
let armTimer: ReturnType<typeof setTimeout> | null = null

const cardVisible = computed(
  () => state.value !== 'idle' && state.value !== 'ok' && props.deviceCount === 0,
)

const softVisible = computed(
  () =>
    softArmed.value &&
    props.deviceCount === 0 &&
    // the blocked/fixing card already carries guidance; tips would be noise
    (state.value === 'idle' || state.value === 'ok'),
)

// Fallback net: zero devices for 20s shows the connection tips no matter what
// the underlying cause is — still scanning, scan already OK (found devices may
// all be filtered out), probe failed silently, third-party firewall, anything.
function syncArmTimer() {
  if (props.deviceCount === 0) {
    if (armTimer === null && !softArmed.value) {
      armTimer = setTimeout(() => {
        softArmed.value = true
        armTimer = null
      }, 20_000)
    }
  } else {
    if (armTimer !== null) {
      clearTimeout(armTimer)
      armTimer = null
    }
    softArmed.value = false
  }
}

watch(() => props.deviceCount, syncArmTimer, { immediate: true })

watch(state, (s) => {
  if (s === 'fixed') emit('fixed')
})

const manualCommands = computed(() => {
  const exe = exePath.value || 'C:\\...\\PlainApp.exe'
  return (
    `netsh advfirewall firewall delete rule name=all dir=in program="${exe}"\n` +
    `netsh advfirewall firewall add rule name="PlainApp" dir=in action=allow program="${exe}" enable=yes profile=any`
  )
})

async function copyCommands() {
  try {
    await navigator.clipboard.writeText(manualCommands.value)
    copied.value = true
    setTimeout(() => {
      copied.value = false
    }, 2000)
  } catch {
    // clipboard unavailable — the pre block is selectable as fallback
  }
}

async function onFix() {
  await fix()
}

onMounted(() => {
  void probe()
})

onBeforeUnmount(() => {
  if (armTimer !== null) {
    clearTimeout(armTimer)
    armTimer = null
  }
})
</script>

<style lang="scss" scoped>
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

.fw-manual {
  margin-top: 10px;
}

.fw-cmd {
  margin: 8px 0 0;
  padding: 8px 10px;
  border-radius: 8px;
  background: var(--md-sys-color-surface-container-high);
  font-size: 0.72rem;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
  user-select: all;
}

.fw-copy {
  margin-top: 6px;
  padding: 2px 10px;
  border: none;
  border-radius: 999px;
  background: transparent;
  color: var(--md-sys-color-primary);
  font-size: 0.75rem;
  cursor: pointer;

  &:hover {
    background: color-mix(in srgb, var(--md-sys-color-primary) 10%, transparent);
  }
}

.soft-hint {
  margin-top: 10px;
  text-align: center;
}

.soft-toggle {
  padding: 0;
  border: none;
  background: transparent;
  color: var(--md-sys-color-primary);
  font-size: 0.8rem;
  cursor: pointer;

  &:hover {
    text-decoration: underline;
  }
}

.soft-list {
  margin: 8px 0 0;
  padding-inline-start: 20px;
  text-align: start;
  font-size: 0.78rem;
  line-height: 1.6;
  color: var(--md-sys-color-on-surface-variant);
}
</style>
