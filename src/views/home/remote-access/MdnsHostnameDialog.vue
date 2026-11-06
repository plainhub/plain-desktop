<template>
  <v-modal width="360px" @close="cancel">
    <template #headline>{{ $t('mdns_hostname') }}</template>
    <template #content>
      <v-text-field
        v-model="value"
        :placeholder="hostname"
        :error="invalid"
        :error-text="invalid ? $t('mdns_hostname_invalid') : ''"
        @keyup.enter="confirm"
      />
    </template>
    <template #actions>
      <v-filled-button :loading="saving" @click="confirm">{{ $t('save') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { popModal } from '@/components/modal'

const props = defineProps<{
  hostname: string
  onSave: (value: string) => Promise<boolean>
}>()

const value = ref(props.hostname)
const invalid = ref(false)
const saving = ref(false)

function cancel() {
  if (saving.value) return
  popModal()
}

async function confirm() {
  const v = value.value.trim()
  invalid.value = v.length === 0 || !v.endsWith('.local')
  if (invalid.value) return
  saving.value = true
  const ok = await props.onSave(v)
  saving.value = false
  if (ok) popModal()
}
</script>
