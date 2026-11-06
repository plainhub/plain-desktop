<template>
  <nav ref="container" class="breadcrumb" :class="{ wrap }" :aria-label="label">
    <span v-for="(item, index) in items" :key="item.path" class="part">
      <span v-if="index" class="separator" aria-hidden="true">/</span>
      <component
        :is="index === items.length - 1 ? 'span' : 'a'"
        v-tooltip="itemTooltip(item, index)"
        class="crumb"
        :href="index === items.length - 1 ? undefined : '#'"
        :aria-current="index === items.length - 1 ? 'page' : undefined"
        @click.stop.prevent="index < items.length - 1 && emit('navigate', item.path)"
      >
        <span class="name">
          <slot name="name" :item="item" :index="index">
            <span class="accessible-name">{{ item.name }}</span>
            <span class="name-start" aria-hidden="true">{{ nameParts(item.name)[0] }}</span
            ><span class="name-end" aria-hidden="true">{{ nameParts(item.name)[1] }}</span>
          </slot>
        </span>
        <slot name="suffix" :item="item" :current="index === items.length - 1" />
      </component>
    </span>
  </nav>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import type { IBreadcrumbItem } from '@/lib/interfaces'

const props = withDefaults(
  defineProps<{
    items: IBreadcrumbItem[]
    label: string
    wrap?: boolean
    tooltip?: (item: IBreadcrumbItem, index: number) => string
  }>(),
  { wrap: false, tooltip: undefined }
)
const emit = defineEmits<{ navigate: [path: string] }>()
const container = ref<HTMLElement | null>(null)

function nameParts(name: string) {
  const characters = Array.from(name)
  return characters.length > 12 ? [characters.slice(0, -6).join(''), characters.slice(-6).join('')] : [name, '']
}

function itemTooltip(item: IBreadcrumbItem, index: number) {
  const details = props.tooltip?.(item, index)
  return details ? `${item.name}\n${details}` : item.name
}

watch(
  () => props.items,
  async () => {
    await nextTick()
    if (!props.wrap && container.value) container.value.scrollLeft = container.value.scrollWidth
  },
  { immediate: true }
)
</script>

<style scoped>
.breadcrumb {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none;
}
.breadcrumb::-webkit-scrollbar {
  display: none;
}
.breadcrumb.wrap {
  flex-wrap: wrap;
  overflow-x: visible;
}
.part {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  max-width: 100%;
  flex-shrink: 0;
}
.crumb {
  display: inline-flex;
  align-items: baseline;
  min-width: 0;
  white-space: nowrap;
}
.name {
  display: inline-flex;
  min-width: 0;
  max-width: 192px;
}
.name-start {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.name-end {
  flex-shrink: 0;
}
.accessible-name {
  position: absolute;
  width: 4px;
  height: 4px;
  overflow: hidden;
  clip-path: inset(50%);
}
.separator {
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
}
[aria-current] {
  color: var(--md-sys-color-on-surface);
  font-weight: 500;
}
</style>
