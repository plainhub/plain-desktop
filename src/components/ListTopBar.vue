<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="allChecked" :indeterminate="!allChecked && checked" @change="toggleAllChecked" />
    <div class="title">
      <span v-if="selectedIds.length">{{ $t('x_selected', { count: (realAllChecked ? total : selectedIds.length).toLocaleString() }) }}</span>
      <span v-else>{{ title }} ({{ total.toLocaleString() }})</span>
      <template v-if="checked">
        <slot name="bulk" />
      </template>
    </div>
    <div v-if="$slots.actions" class="actions">
      <slot name="actions" />
    </div>
  </div>
  <all-checked-alert
    v-if="alert"
    :limit="alert.limit"
    :total="total"
    :all-checked-alert-visible="alert.visible"
    :real-all-checked="realAllChecked"
    :select-real-all="alert.selectRealAll"
    :clear-selection="alert.clearSelection"
  />
</template>

<script setup lang="ts">
defineProps<{
  title: string
  total: number
  selectedIds: string[]
  checked: boolean
  allChecked: boolean
  realAllChecked: boolean
  toggleAllChecked: () => void
  alert?: {
    limit: number
    visible: boolean
    selectRealAll: () => void
    clearSelection: () => void
  }
}>()
</script>
