<template>
  <section
    class="media-item"
    :class="{ selected, selecting: shiftSelected }"
    @click.stop="$emit('item-click', $event)"
    @mouseenter.stop="$emit('item-mouse-enter', $event)"
  >
    <slot name="thumbnail" />
    <v-icon-button class="btn-checkbox" @click.stop="$emit('toggle-select', $event)">
      <v-check-circle :checked="isChecked" />
    </v-icon-button>
    <v-icon-button v-if="checked" v-tooltip="$t('open')" class="btn-zoom sm" @click.stop="$emit('view')">
      <i-material-symbols:zoom-in-rounded />
    </v-icon-button>
    <div class="info" :class="{ 'has-tags': item.tags.length > 0 }">
      <item-tags :tags="item.tags" :type="dataType" />
      <span class="right"><slot name="info-right" /></span>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { DataType } from '@/lib/data'
import type { ITag } from '@/lib/interfaces'

const props = defineProps<{
  item: { id: string; tags: ITag[] }
  checked: boolean
  selected: boolean
  shiftSelected: boolean
  shouldSelect: boolean
  dataType: DataType
}>()

const isChecked = computed(() => (props.shiftSelected ? props.shouldSelect : props.selected))

defineEmits<{
  (e: 'item-click', event: MouseEvent): void
  (e: 'item-mouse-enter', event: MouseEvent): void
  (e: 'toggle-select', event: MouseEvent): void
  (e: 'view'): void
}>()
</script>
