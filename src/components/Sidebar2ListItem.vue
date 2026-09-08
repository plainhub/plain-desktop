<template>
  <a class="item-link" :href="href">
    <article
      class="sidebar2-item selectable-card"
      :class="{ selected, selecting }"
      @click.stop.prevent="$emit('item-click', $event)"
      @mouseenter.stop="$emit('mouse-over', $event)"
    >
      <div class="title">
        <v-checkbox class="checkbox" touch-target="wrapper" :checked="checkboxChecked" @click.stop="$emit('toggle-select', $event)" />
        <div class="text"><slot name="title" /></div>
      </div>
      <div class="subtitle">
        <span class="number"><field-id :id="index + 1" :raw="item" /></span>
        <div class="info">
          <slot name="info" />
        </div>
      </div>
      <slot name="end" />
    </article>
  </a>
</template>

<script setup lang="ts">
import type { IData } from '@/lib/interfaces'

interface Props {
  item: IData
  index: number
  href: string
  selected: boolean
  selecting: boolean
  checkboxChecked: boolean
}

defineProps<Props>()

defineEmits<{
  'item-click': [event: MouseEvent]
  'mouse-over': [event: MouseEvent]
  'toggle-select': [event: MouseEvent]
}>()
</script>

<style lang="scss">
.sidebar2-item {
  margin: 0 16px 8px 16px;
  display: grid;
  box-sizing: border-box;
  border-radius: 8px;
  grid-template-areas:
    'title image'
    'subtitle image';
  grid-template-columns: 1fr auto;
  &:hover {
    cursor: pointer;
  }
  .title {
    grid-area: title;
    display: flex;
    .checkbox {
      margin-inline-start: 4px;
    }
    .text {
      font-weight: 500;
      flex: 1;
      width: 0;
      margin-block: 8px;
      margin-inline-end: 12px;
      word-break: break-word;
    }
  }
  .subtitle {
    font-size: 0.875rem;
    grid-area: subtitle;
    display: flex;
    flex-direction: row;
    align-items: end;
    margin-block-end: 12px;
    margin-inline-end: 16px;
    margin-inline-start: 4px;
    .number {
      min-width: 40px;
      text-align: center;
    }
    .info {
      display: flex;
      gap: 4px;
      flex: 1;
      flex-flow: wrap;
      align-items: center;
    }
  }
  .image {
    width: 50px;
    height: 50px;
    grid-area: image;
    object-fit: cover;
    border-radius: 8px;
    margin-block: 12px;
    margin-inline-end: 12px;
  }
}
</style>
