<template>
  <v-modal width="360px" @close="popModal">
    <template #headline>{{ $t('remote_access_qr') }}</template>
    <template #content>
      <img v-if="address()" class="address-qr" :alt="address()" :src="svg" />
      <div v-if="address()" class="qr-address">
        <code>{{ address() }}</code
        ><v-copy-button :text="address()" />
      </div>
      <p v-if="!address()">{{ $t('remote_access_addresses_unavailable') }}</p>
    </template>
    <template #actions
      ><v-text-button @click="popModal">{{ $t('close') }}</v-text-button></template
    >
  </v-modal>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { popModal } from '@/components/modal'
import { qrCodeSvg } from '@/lib/qr'

const props = defineProps<{ address: () => string }>()
const svg = computed(() => (props.address() ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(qrCodeSvg(props.address(), { border: 4 }))}` : ''))
</script>

<style scoped>
.address-qr {
  display: block;
  width: 100%;
  height: auto;
  max-width: 240px;
  margin: 0 auto;
}
.qr-address {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-top: 16px;
}
.qr-address code {
  min-width: 0;
  flex: 1;
  overflow-wrap: anywhere;
}
</style>
