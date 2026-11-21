<template>
  <section
    v-if="!isPhone"
    class="file-item selectable-card"
    :class="{ selected, selecting }"
    @click.stop="emit('click', $event)"
    @mouseenter.stop="emit('mouseenter', $event)"
  >
    <div class="start">
      <v-checkbox class="checkbox" touch-target="wrapper" :label="item.name" :checked="checked" @click.stop="emit('checkboxClick', $event)" />
      <span class="number"><field-id :id="index + 1" :raw="item" /></span>
    </div>
    <slot name="image" />
    <div class="title">{{ item.name }} <OnlinePreviewIcon :name="item.name" :is-dir="item.isDir" /></div>
    <div class="subtitle"><FileItemDetails :item="item" /></div>
    <slot name="actions" />
  </section>
  <ListItemPhone
    v-else
    :is-selected="selected"
    :is-selecting="selecting"
    :checkbox-checked="checked"
    :checkbox-label="item.name"
    @click="emit('click', $event)"
    @mouseenter.stop="emit('mouseenter', $event)"
    @checkbox-click="emit('checkboxClick', $event)"
  >
    <template #image><slot name="image" /></template>
    <template #title>{{ item.name }} <OnlinePreviewIcon :name="item.name" :is-dir="item.isDir" /></template>
    <template #subtitle><FileItemDetails :item="item" /></template>
    <template #actions><slot name="actions" /></template>
  </ListItemPhone>
</template>

<script setup lang="ts">
import FileItemDetails, { type FileRow } from './FileItemDetails.vue'

defineProps<{
  item: FileRow
  index: number
  isPhone: boolean
  selected: boolean
  selecting?: boolean
  checked: boolean
}>()

const emit = defineEmits<{
  click: [event: MouseEvent]
  mouseenter: [event: MouseEvent]
  checkboxClick: [event: MouseEvent]
}>()
</script>

<style scoped>
.list-item-phone {
  margin-block-end: 8px;
}
</style>
