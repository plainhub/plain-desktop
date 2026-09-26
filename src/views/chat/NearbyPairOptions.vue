<template>
  <div class="pair-options">
    <p>{{ $t('pairing_access_prompt') }}</p>
    <label class="pair-option" :class="{ selected: mode === 'chat' }">
      <input v-model="mode" type="radio" value="chat" name="pair-access" />
      <span>
        <strong>{{ $t('pairing_chat_only') }}</strong>
        <small>{{ $t('pairing_chat_only_hint') }}</small>
      </span>
    </label>
    <label class="pair-option" :class="{ selected: mode === 'control' }">
      <input v-model="mode" type="radio" value="control" name="pair-access" />
      <span>
        <strong>{{ $t('pairing_full_control') }}</strong>
        <small>{{ $t('pairing_full_control_hint') }}</small>
      </span>
    </label>
    <div class="pair-actions">
      <v-outlined-button @click="emit('cancel')">{{ $t('cancel') }}</v-outlined-button>
      <v-filled-button @click="emit('confirm', mode)">{{ $t('pairing_start') }}</v-filled-button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

const emit = defineEmits<{ cancel: []; confirm: [mode: 'chat' | 'control'] }>()
const mode = ref<'chat' | 'control'>('control')
</script>

<style scoped lang="scss">
.pair-options {
  display: flex;
  flex-direction: column;
  gap: 12px;

  p {
    margin: 0 0 4px;
  }
}

.pair-option {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 12px;
  cursor: pointer;

  &.selected {
    border-color: var(--md-sys-color-primary);
    background: var(--md-sys-color-primary-container);
  }

  span {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  strong {
    color: var(--md-sys-color-on-surface);
  }

  small {
    color: var(--md-sys-color-on-surface-variant);
  }
}

.pair-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 12px;
}
</style>
