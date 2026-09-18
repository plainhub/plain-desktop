<template>
  <v-chip-select
    :model-value="modelValue"
    :options="options"
    :aria-label="$t('image_quality')"
    @update:model-value="onPick"
  />
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import VChipSelect from '@/components/base/VChipSelect.vue'
import type { VSelectOption } from '@/components/base/VSelect.vue'

defineProps<{
  modelValue: 'fast' | 'original'
}>()

const emit = defineEmits<{
  'update:modelValue': [value: 'fast' | 'original']
}>()

const { t } = useI18n()

const options = computed<VSelectOption[]>(() => [
  { value: 'fast', label: t('image_quality_fast') },
  { value: 'original', label: t('image_quality_original') },
])

function onPick(value: string | number) {
  if (value === 'fast' || value === 'original') emit('update:modelValue', value)
}
</script>
