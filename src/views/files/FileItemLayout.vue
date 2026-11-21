<template>
  <section
    v-if="!isPhone"
    class="file-item selectable-card"
    :class="{ selected, selecting }"
    @click.stop="emit('click', $event)"
    @mouseenter.stop="emit('mouseenter', $event)"
  >
    <div class="start">
      <v-checkbox class="checkbox" touch-target="wrapper" :label="name" :checked="checked" @click.stop="emit('checkboxClick', $event)" />
      <span class="number"><field-id :id="index + 1" :raw="item" /></span>
    </div>
    <slot name="image" />
    <div class="title"><slot name="title" /></div>
    <div class="subtitle"><slot name="subtitle" /></div>
    <slot name="actions" />
  </section>
  <ListItemPhone
    v-else
    :is-selected="selected"
    :is-selecting="selecting"
    :checkbox-checked="checked"
    :checkbox-label="name"
    @click="emit('click', $event)"
    @mouseenter.stop="emit('mouseenter', $event)"
    @checkbox-click="emit('checkboxClick', $event)"
  >
    <template #image><slot name="image" /></template>
    <template #title><slot name="title" /></template>
    <template #subtitle><slot name="subtitle" /></template>
    <template #actions><slot name="actions" /></template>
  </ListItemPhone>
</template>

<script setup lang="ts">
defineProps<{
  item: object
  name: string
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
