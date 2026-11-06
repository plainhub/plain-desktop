<template>
  <div class="remote-addresses">
    <div class="protocol-row">
      <div class="protocol-tabs" role="tablist" :aria-label="$t('remote_access_protocol')">
        <button
          v-for="kind in protocols"
          :id="`remote-${kind}-tab`"
          :key="kind"
          type="button"
          role="tab"
          :aria-selected="protocol === kind"
          aria-controls="remote-address-panel"
          @click="selectProtocol(kind)"
        >
          {{ kind.toUpperCase() }}
        </button>
      </div>
      <v-text-button :disabled="!port" :aria-label="$t('edit_port') + ': ' + protocol.toUpperCase()" @click="$emit('editPort', protocol)">
        {{ $t('port') }} {{ port || '—' }}<i-material-symbols:edit-rounded aria-hidden="true" />
      </v-text-button>
    </div>
    <div id="remote-address-panel" role="tabpanel" :aria-labelledby="`remote-${protocol}-tab`">
      <p v-if="!enabled">{{ $t('remote_access_off') }}</p>
      <p v-else-if="loadingIps || !port">{{ $t('loading') }}</p>
      <div v-else-if="addressError" role="alert">
        {{ $t('remote_access_addresses_unavailable') }}<v-text-button @click="$emit('retry')">{{ $t('retry') }}</v-text-button>
      </div>
      <template v-else-if="address">
        <div class="address-row">
          <code>{{ address }}</code>
          <v-copy-button v-tooltip="$t('copy')" class="address-action" :text="address" />
          <v-icon-button v-tooltip="$t('remote_access_qr')" :aria-label="$t('remote_access_qr')" @click="showQr"><i-material-symbols:qr-code-2-rounded /></v-icon-button>
        </div>
        <button v-if="alternatives.length" type="button" class="other-addresses" :aria-expanded="expanded" aria-controls="remote-other-addresses" @click="expanded = !expanded">
          {{ $t('remote_access_other_addresses') }}<i-material-symbols:expand-less-rounded v-if="expanded" /><i-material-symbols:expand-more-rounded v-else />
        </button>
        <div v-if="expanded" id="remote-other-addresses">
          <div v-for="alternative in alternatives" :key="alternative" class="address-row">
            <button type="button" class="address-choice" @click="selectAddress(alternative)">
              <code>{{ alternative }}</code>
            </button>
            <v-copy-button class="address-action" :text="alternative" />
          </div>
        </div>
      </template>
      <p v-else>{{ $t('remote_access_addresses_unavailable') }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { pushModal } from '@/components/modal'
import AddressQrDialog from './AddressQrDialog.vue'

const props = defineProps<{
  httpPort: number
  httpsPort: number
  httpAddresses: string[]
  httpsAddresses: string[]
  enabled: boolean
  loadingIps: boolean
  addressError: boolean
}>()
defineEmits<{ editPort: [kind: 'http' | 'https']; retry: [] }>()
const protocols = ['http', 'https'] as const
const protocol = ref<'http' | 'https'>('http')
const selectedHost = ref('')
const expanded = ref(false)
const port = computed(() => (protocol.value === 'http' ? props.httpPort : props.httpsPort))
const addresses = computed(() => (port.value ? (protocol.value === 'http' ? props.httpAddresses : props.httpsAddresses) : []))
const address = computed(() => addresses.value.find((value) => new URL(value).hostname === selectedHost.value) ?? addresses.value[0] ?? '')
const alternatives = computed(() => addresses.value.filter((value) => value !== address.value))
function selectProtocol(kind: 'http' | 'https') {
  protocol.value = kind
  expanded.value = false
}
function selectAddress(value: string) {
  selectedHost.value = new URL(value).hostname
  expanded.value = false
}
function showQr() {
  void pushModal(AddressQrDialog, { address: () => address.value })
}
</script>

<style scoped lang="scss" src="./RemoteAccessAddresses.scss"></style>
