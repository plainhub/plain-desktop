<template>
  <v-modal width="360px" @close="cancel">
    <template #headline>{{ $t('edit_port') }}</template>
    <template #content>
      <div class="port-dialog-body">
        <v-select v-model="value" :options="options" :placeholder="$t('port')" />
      </div>
    </template>
    <template #actions>
      <v-filled-button :loading="saving" @click="confirm">{{ $t('confirm') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { popModal } from '@/components/modal'
import type { VSelectOption } from '@/components/base/VSelect.vue'

const props = defineProps<{
  current: number
  options: VSelectOption[]
  onSave: (port: number) => Promise<boolean>
}>()

const value = ref<number | undefined>(props.current)
const saving = ref(false)

function cancel() {
  if (saving.value) return
  popModal()
}

async function confirm() {
  if (value.value == null) return
  saving.value = true
  const ok = await props.onSave(value.value)
  saving.value = false
  if (ok) popModal()
}
</script>

<style lang="scss" scoped>
.port-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
</style>
