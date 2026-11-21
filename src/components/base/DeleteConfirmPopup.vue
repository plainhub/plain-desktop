<template>
  <v-dropdown-menu :key="confirming" v-model="open" :anchor="anchor">
    <div v-if="confirming" class="delete-confirm-popup" role="dialog" :aria-labelledby="`${anchor}-delete-message`">
      <div :id="`${anchor}-delete-message`" class="delete-confirm-popup-msg">
        {{ confirmText ?? $t(messageKey ?? (count === undefined ? 'confirm_to_delete_name' : 'confirm_to_delete'), { name, count }) }}
      </div>
      <div class="delete-confirm-popup-actions">
        <v-outlined-button @click.stop="cancel">{{ $t('cancel') }}</v-outlined-button>
        <v-filled-button class="danger" :loading="loading" @click.stop="$emit('confirm')">{{ $t('confirm') }}</v-filled-button>
      </div>
    </div>
    <slot v-else name="menu" />
  </v-dropdown-menu>
</template>

<script setup lang="ts">
import { onDeactivated } from 'vue'

const open = defineModel<boolean>({ default: false })
withDefaults(defineProps<{
  anchor: string
  confirming?: boolean
  name?: string
  count?: number
  loading?: boolean
  messageKey?: string
  confirmText?: string
}>(), { confirming: true, name: undefined, count: undefined, messageKey: undefined, confirmText: undefined })
const emit = defineEmits<{ confirm: []; cancel: [] }>()
const slots = defineSlots<{ menu?: () => unknown }>()

onDeactivated(() => { if (open.value) open.value = false })

function cancel() {
  if (slots.menu) emit('cancel')
  else open.value = false
}
</script>

<style scoped lang="scss">
.delete-confirm-popup {
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 180px;
}

.delete-confirm-popup-msg {
  font-size: 13px;
  color: var(--md-sys-color-on-surface);
  line-height: 1.4;
  white-space: break-spaces;
  word-break: break-all;
}

.delete-confirm-popup-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}
</style>
