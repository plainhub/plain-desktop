<template>
  <v-dropdown v-model="open">
    <template #trigger>
      <button v-tooltip="tooltip" class="btn-icon" @click.prevent>
        <i-material-symbols:volume-off-rounded v-if="modelValue === 0" />
        <i-material-symbols:volume-down-rounded v-else-if="modelValue < 0.5" />
        <i-material-symbols:volume-up-rounded v-else />
      </button>
    </template>
    <div class="sound-popup">
      <input
        :value="modelValue"
        type="range"
        min="0"
        max="1"
        step="0.05"
        class="volume-slider"
        @input="$emit('update:modelValue', +($event.target as HTMLInputElement).value)"
      />
    </div>
  </v-dropdown>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ modelValue: number }>()
defineEmits<{ (e: 'update:modelValue', v: number): void }>()

const { t } = useI18n()
const open = ref(false)

const tooltip = computed(() =>
  props.modelValue === 0 ? t('notification_sound_off') : t('notification_sound_on')
)
</script>

<style lang="scss" scoped>
.sound-popup {
  padding: 8px 12px;
  display: flex;
  align-items: center;
}

.volume-slider {
  width: 96px;
  accent-color: var(--md-sys-color-primary);
  cursor: pointer;
}
</style>
