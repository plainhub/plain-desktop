<template>
  <div class="actions">
    <template v-if="filter.trash">
      <template v-if="!confirming">
        <v-icon-button v-tooltip="$t('delete')" class="sm" @click.stop="confirming = true">
          <i-material-symbols:delete-forever-outline-rounded />
        </v-icon-button>
        <v-icon-button v-tooltip="$t('restore')" class="sm" :loading="restoreLoading(`ids:${item.id}`)" @click.stop="restore(dataType, `ids:${item.id}`)">
          <i-material-symbols:restore-from-trash-outline-rounded />
        </v-icon-button>
        <v-icon-button v-tooltip="$t('download')" class="sm" @click.stop="$emit('download-file', item.path)">
          <i-material-symbols:download-rounded />
        </v-icon-button>
      </template>
      <inline-delete-confirm v-else :name="item.title" @confirm="onConfirmDelete" @cancel="confirming = false" />
    </template>
    <template v-else>
      <v-icon-button
        v-if="hasMediaTrash(app)"
        v-tooltip="$t('move_to_trash')"
        class="sm"
        :loading="trashLoading(`ids:${item.id}`)"
        @click.stop="trash(dataType, `ids:${item.id}`)"
      >
        <i-material-symbols:delete-outline-rounded />
      </v-icon-button>
      <v-icon-button v-else v-tooltip="$t('delete')" class="sm" @click.stop="$emit('delete-item', item)">
        <i-material-symbols:delete-forever-outline-rounded />
      </v-icon-button>
      <v-icon-button v-tooltip="$t('download')" class="sm" @click.stop="$emit('download-file', item.path)">
        <i-material-symbols:download-rounded />
      </v-icon-button>
      <TagRelationsDropdown :type="dataType" :tags="tags" :item="{ key: item.id, title: item.title, size: item.size }" :selected="item.tags" />

      <v-dropdown v-model="infoVisible">
        <template #trigger>
          <v-icon-button v-tooltip="$t('info')" class="sm">
            <i-material-symbols:info-outline-rounded />
          </v-icon-button>
        </template>
        <section class="card card-info">
          <div class="key-value vertical">
            <div class="key">{{ $t('path') }}</div>
            <div class="value">{{ item.path }}</div>
          </div>
          <div class="key-value">
            <div class="key">{{ $t('size') }}</div>
            <div class="value">{{ formatFileSize(item.size) }}</div>
          </div>
          <div class="key-value">
            <div class="key">{{ $t('updated_at') }}</div>
            <div class="value">{{ formatDateTimeFull(item.updatedAt) }}</div>
          </div>
        </section>
      </v-dropdown>

      <v-dropdown v-model="moreVisible">
        <template #trigger>
          <v-icon-button v-tooltip="$t('actions')" class="sm">
            <i-material-symbols:more-vert />
          </v-icon-button>
        </template>
        <div class="dropdown-item" @click.stop="$emit('open-file', item); moreVisible = false">
          {{ $t('open') }}
        </div>
        <div class="dropdown-item" @click.stop="$emit('rename-item', item); moreVisible = false">
          {{ $t('rename') }}
        </div>
        <div class="dropdown-item" @click.stop="$emit('duplicate-item', item); moreVisible = false">
          {{ $t('duplicate') }}
        </div>
      </v-dropdown>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { IDoc, IFilter, IApp, ITag } from '@/lib/interfaces'
import { DataType } from '@/lib/data'
import { hasMediaTrash } from '@/lib/feature'
import { formatFileSize, formatDateTimeFull } from '@/lib/format'

const props = defineProps<{
  item: IDoc
  filter: IFilter
  app: IApp
  dataType: DataType
  tags: ITag[]
  trashLoading: (query: string) => boolean
  restoreLoading: (query: string) => boolean
  trash: (dataType: DataType, query: string) => void
  restore: (dataType: DataType, query: string) => void
  deleteItem: (dataType: DataType, item: any) => void
}>()

defineEmits<{
  'download-file': [path: string]
  'delete-item': [item: IDoc]
  'open-file': [item: IDoc]
  'rename-item': [item: IDoc]
  'duplicate-item': [item: IDoc]
}>()

const infoVisible = ref(false)
const moreVisible = ref(false)
const confirming = ref(false)

function onConfirmDelete() {
  props.deleteItem(props.dataType, props.item)
  confirming.value = false
}
</script>
