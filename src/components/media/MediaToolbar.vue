<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="allChecked" :indeterminate="!allChecked && checked" @change="$emit('toggleAllChecked', $event)" />
    <div class="title">
      <span v-if="selectedCount > 0">{{ $t('x_selected', { count: (realAllChecked ? total : selectedCount).toLocaleString() }) }}</span>
      <span v-else>{{ $t(pageTitle) }} ({{ total.toLocaleString() }})</span>
      <template v-if="checked">
        <template v-if="filterTrash">
          <bulk-delete-button :confirming="confirmingDelete" :count="deleteCount" :loading="deleteLoading" @click="$emit('delete')" @confirm="$emit('confirm')" @cancel="$emit('cancel')" />
          <v-icon-button v-tooltip="$t('restore')" :loading="restoreQueryLoading" @click.stop="$emit('restore')"><i-material-symbols:restore-from-trash-outline-rounded /></v-icon-button>
          <bulk-download-button :single="realAllChecked || selectedCount === 1" @download="$emit('download')" @download-each="$emit('download-each')" @download-zip="$emit('download-zip')" />
        </template>
        <template v-else>
            <v-icon-button v-if="canTrash" v-tooltip="$t('move_to_trash')" :loading="trashQueryLoading" @click.stop="$emit('trash')"><i-material-symbols:delete-outline-rounded /></v-icon-button>
            <bulk-delete-button v-else :confirming="confirmingDelete" :count="deleteCount" :loading="deleteLoading" @click="$emit('delete')" @confirm="$emit('confirm')" @cancel="$emit('cancel')" />
            <slot name="tag-action">
              <v-icon-button v-tooltip="$t('add_to_tags')" @click.stop="$emit('addToTags')"><i-material-symbols:label-outline-rounded /></v-icon-button>
            </slot>
          <bulk-download-button :single="realAllChecked || selectedCount === 1" @download="$emit('download')" @download-each="$emit('download-each')" @download-zip="$emit('download-zip')" />
          <slot name="extra-actions" />
        </template>
      </template>
    </div>
    <div class="actions"><slot name="actions" /></div>
  </div>
  <div v-if="showSecondary" class="secondary-actions"><slot name="secondary" /></div>
  <all-checked-alert
    :limit="limit"
    :total="total"
    :all-checked-alert-visible="allCheckedAlertVisible"
    :real-all-checked="realAllChecked"
    :select-real-all="() => $emit('selectRealAll')"
    :clear-selection="() => $emit('clearSelection')"
  />
</template>

<script setup lang="ts">
defineProps<{
  pageTitle: string
  selectedCount: number
  allChecked: boolean
  checked: boolean
  realAllChecked: boolean
  total: number
  filterTrash: boolean
  canTrash: boolean
  restoreQueryLoading: boolean
  trashQueryLoading: boolean
  limit: number
  allCheckedAlertVisible: boolean
  showSecondary: boolean
  confirmingDelete: boolean
  deleteCount: number
  deleteLoading: boolean
}>()

defineEmits<{
  toggleAllChecked: [event: Event]
  confirm: []
  cancel: []
  delete: []
  restore: []
  download: []
  'download-each': []
  'download-zip': []
  trash: []
  addToTags: []
  selectRealAll: []
  clearSelection: []
}>()
</script>
