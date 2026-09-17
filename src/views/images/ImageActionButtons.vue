<template>
  <div class="actions">
    <template v-if="!confirming">
      <template v-if="filter.trash">
        <v-icon-button v-tooltip="$t('delete')" class="sm" @click.stop="confirming = true">
            <i-material-symbols:delete-forever-outline-rounded />
        </v-icon-button>
        <v-icon-button v-tooltip="$t('restore')" class="sm" :loading="restoreLoading(`ids:${item.id}`)" @click.stop="restore(dataType, `ids:${item.id}`)">
            <i-material-symbols:restore-from-trash-outline-rounded />
        </v-icon-button>
        <v-icon-button v-tooltip="$t('download')" class="sm" @click.stop="downloadFile(item.path, getFileName(item.path).replace(' ', '-'))">
            <i-material-symbols:download-rounded />
        </v-icon-button>
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
          <v-icon-button v-else v-tooltip="$t('delete')" class="sm" @click.stop="confirming = true">
              <i-material-symbols:delete-forever-outline-rounded />
          </v-icon-button>
          <v-icon-button v-tooltip="$t('download')" class="sm" @click.stop="downloadFile(item.path, getFileName(item.path).replace(' ', '-'))">
              <i-material-symbols:download-rounded />
          </v-icon-button>
          <TagRelationsDropdown :type="dataType" :tags="tags" :item="{ key: item.id, title: item.title, size: item.size }" :selected="item.tags" />
      </template>
    </template>
    <inline-delete-confirm v-else :name="item.title" @confirm="onConfirmDelete" @cancel="confirming = false" />
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { IImageItem, IFilter, ITag } from '@/lib/interfaces'
import { DataType } from '@/lib/data'
import { getFileName } from '@/lib/api/file'
import { hasMediaTrash } from '@/lib/feature'
import { useI18n } from 'vue-i18n'

const { t: $t } = useI18n()

interface Props {
  item: IImageItem
  filter: IFilter
  dataType: DataType
  tags: ITag[]
  app: any
  // Functions passed from parent
  deleteItem: (dataType: DataType, item: IImageItem) => void
  restore: (dataType: DataType, query: string) => void
  downloadFile: (path: string, fileName: string) => void
  trash: (dataType: DataType, query: string) => void
  restoreLoading: (query: string) => boolean
  trashLoading: (query: string) => boolean
}

const props = defineProps<Props>()

const confirming = ref(false)

function onConfirmDelete() {
  props.deleteItem(props.dataType, props.item)
  confirming.value = false
}
</script>