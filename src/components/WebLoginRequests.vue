<template>
  <v-modal v-if="current" @close="decide(false)">
    <template #headline>{{ $t('login.approval_title') }}</template>
    <template #content>
      <ul class="card list-items">
        <v-list-item :title="$t('browser')" :value="`${current.request.browserName} ${current.request.browserVersion}`" />
        <v-list-item :title="$t('os')" :value="`${current.request.osName} ${current.request.osVersion}`" />
        <v-list-item :title="$t('ip_address')" :value="current.clientIp" />
      </ul>
      <p v-if="current.request.verificationCode">{{ $t('login.compare_code') }}</p>
      <code v-if="current.request.verificationCode">{{ current.request.verificationCode }}</code>
      <p v-if="error" role="alert">{{ error }}</p>
    </template>
    <template #actions>
      <v-outlined-button :disabled="busy" @click="decide(false)">{{ $t('deny') }}</v-outlined-button>
      <v-filled-button :disabled="busy" :loading="busy" @click="decide(true)">{{ $t('allow') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { subscribeWebLoginRequests, decideWebLogin, type WebLoginRequest } from '@/lib/device/web-login-requests'
import type { NativeEventStream } from '@/lib/api/native-events'

const requests = ref<WebLoginRequest[]>([])
const current = computed(() => requests.value[0])
const busy = ref(false)
const error = ref('')
const { t } = useI18n()
let stream: NativeEventStream | undefined
onMounted(() => {
  stream = subscribeWebLoginRequests((request) => {
    requests.value = requests.value.filter((pending) => pending.clientId !== request.clientId)
    requests.value.push(request)
  }, () => { requests.value = [] }, (id) => {
    requests.value = requests.value.filter((request) => request.requestId !== id)
  })
})
onBeforeUnmount(() => stream?.close())

async function decide(allowed: boolean) {
  if (busy.value || !current.value) return
  const requestId = current.value.requestId
  busy.value = true
  error.value = ''
  try {
    await decideWebLogin(requestId, allowed)
    requests.value = requests.value.filter((request) => request.requestId !== requestId)
  } catch (reason) {
    if (requests.value.some((request) => request.requestId === requestId)) {
      error.value = t(/expired|no longer pending/.test(String(reason)) ? 'login.login_expired' : 'login.failed')
    }
  } finally { busy.value = false }
}
</script>
