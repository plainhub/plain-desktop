<template>
  <v-modal @close="$emit('close')">
    <template #headline>{{ $t('session_management.token_created') }}</template>
    <template #content>
      <p>{{ $t('session_management.copy_new_token') }}</p>
      <dl>
        <div><dt>{{ $t('client_id') }}</dt><dd><code>{{ credential.clientId }}</code><v-copy-button :text="credential.clientId" /></dd></div>
        <div><dt>{{ $t('session_management.token') }}</dt><dd><code>{{ credential.token }}</code><v-copy-button :text="credential.token" /></dd></div>
      </dl>
    </template>
    <template #actions><v-filled-button @click="$emit('close')">{{ $t('close') }}</v-filled-button></template>
  </v-modal>
</template>

<script setup lang="ts">
import type { CreatedApiToken } from './api'
defineProps<{ credential: CreatedApiToken }>()
defineEmits<{ close: [] }>()
</script>

<style scoped>
dl { display: flex; flex-direction: column; gap: 16px; }
dt { color: var(--md-sys-color-on-surface-variant); }
dd { margin: 4px 0 0; display: flex; align-items: center; gap: 8px; }
code { overflow-wrap: anywhere; }
</style>
