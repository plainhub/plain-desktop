<template>
  <div v-if="visible" class="http-security-notice">
    <span>
      {{ $t('http_security_warning') }}
      <a v-if="httpsUrl" :href="httpsUrl" rel="noreferrer">{{ $t('http_security_switch') }}</a>
      <template v-else>{{ $t('http_security_switch') }}</template>
    </span>
    <span v-if="httpsUrl" class="certificate-note">{{ $t('http_security_certificate') }}</span>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { requestInit } from '@/lib/api/init'
import { getApiBaseUrl } from '@/lib/api/api'
import { httpsPageUrl, isHttpBrowser } from '@/lib/http-security'

const route = useRoute()
const visible = isHttpBrowser(window.location.protocol, __IS_TAURI__)
const httpsPort = ref<number>()
const httpsUrl = computed(() => httpsPageUrl(`${window.location.origin}${route.fullPath}`, httpsPort.value))

onMounted(async () => {
  if (!visible) return
  try {
    if (new URL(getApiBaseUrl()).origin !== window.location.origin) return
    const result = await requestInit()
    if (result.status === 200) httpsPort.value = result.data?.httpsPort
  } catch {
    // Keep the HTTP warning visible when the HTTPS listener cannot be discovered.
  }
})
</script>

<style scoped>
.http-security-notice {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  justify-content: center;
  gap: 4px 8px;
  box-sizing: border-box;
  width: calc(100% - 32px);
  max-width: 400px;
  margin: 16px auto 0;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.8rem;
  text-align: center;
  overflow-wrap: anywhere;
}

.certificate-note {
  flex-basis: 100%;
}

@media print {
  .http-security-notice {
    display: none;
  }
}
</style>
