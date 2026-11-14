<template>
  <div ref="listEl" class="features-list">
    <div class="drag-hint">
      {{ $t('drag_to_reorder') }}
    </div>

    <div
      v-for="(item, idx) in displayList"
      :key="item.feature.id"
      class="feature-row"
      :class="{ enabled: item.enabled, dragging: fromIndex === idx }"
      :style="itemStyle(idx)"
      @pointerdown="item.enabled && start($event, idx)"
    >
      <div class="feat-order">{{ item.enabled ? idx + 1 : '—' }}</div>
      <component :is="item.feature.icon" class="feat-icon" />
      <span class="feat-name">{{ $t(item.feature.titleKey) }}</span>
      <v-switch :label="$t(item.feature.titleKey)" :checked="item.enabled" :disabled="item.enabled && enabledIds.length === 1" @change="toggle(item.feature.id)" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, type Component } from 'vue'
import { useDragSort } from '@/hooks/use-drag-sort'

interface CustomizableFeature {
  id: string
  icon: Component
  titleKey: string
}

const props = defineProps<{
  features: CustomizableFeature[]
  modelValue: string[]
}>()

const emit = defineEmits<{
  'update:modelValue': [ids: string[]]
}>()

const enabledIds = computed({
  get: () => props.modelValue.filter((id) => props.features.some((f) => f.id === id)),
  set: (ids: string[]) => emit('update:modelValue', ids),
})

const displayList = computed(() => {
  const featureMap = new Map(props.features.map((feature) => [feature.id, feature]))
  const enabled = enabledIds.value
    .map((id) => featureMap.get(id))
    .filter((feature): feature is CustomizableFeature => !!feature)
    .map((feature) => ({ feature, enabled: true }))
  const disabled = props.features
    .filter((feature) => !enabledIds.value.includes(feature.id))
    .map((feature) => ({ feature, enabled: false }))
  return [...enabled, ...disabled]
})

const listEl = ref<HTMLElement>()

const { fromIndex, itemStyle, start } = useDragSort({
  containerRef: listEl,
  itemSelector: '.feature-row',
  onEnd(from, to) {
    if (to >= enabledIds.value.length) return
    const ids = [...enabledIds.value]
    const [moved] = ids.splice(from, 1)
    if (!moved) return
    ids.splice(to, 0, moved)
    enabledIds.value = ids
  },
})

function toggle(id: string) {
  enabledIds.value = enabledIds.value.includes(id)
    ? enabledIds.value.filter((value) => value !== id)
    : [...enabledIds.value, id]
}
</script>

<style lang="scss" scoped src="./FeatureSortToggleList.scss"></style>
