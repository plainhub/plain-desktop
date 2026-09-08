<template>
  <section
    v-if="!isPhone"
    class="file-item selectable-card"
    :class="{ selected: selectedIds.includes(item.id), selecting: shiftEffectingIds.includes(item.id) }"
    @click.stop="handleItemClick($event, item, index, () => clickItem(item))"
    @mouseenter.stop="handleMouseOver($event, index)"
  >
    <div class="start">
      <v-checkbox v-if="shiftEffectingIds.includes(item.id)" class="checkbox" touch-target="wrapper" :checked="shouldSelect" @click.stop="toggleSelect($event, item, index)" />
      <v-checkbox v-else class="checkbox" touch-target="wrapper" :checked="selectedIds.includes(item.id)" @click.stop="toggleSelect($event, item, index)" />
      <span class="number"><field-id :id="index + 1" :raw="item" /></span>
    </div>
    
    <div class="image" @click="viewItem($event, item)">
      <FileThumb
        :is-dir="item.isDir"
        :thumb-url="thumbUrl"
        :extension="item.extension"
        :thumb-error="imageErrorIds.includes(item.id)"
        :ext-error="extensionImageErrorIds.includes(item.id)"
        :on-thumb-error="() => onImageError(item.id)"
        :on-ext-error="() => onExtensionImageError(item.id)"
      />
    </div>
    
    <div class="title">
      {{ item.name }}
      <OnlinePreviewIcon :name="item.name" :is-dir="item.isDir" />
    </div>
    
    <div class="subtitle">
      <span v-if="item.isDir">{{ $t('x_items', item.children || 0) }}</span>
      <span v-else>{{ formatFileSize(item.size) }}</span>
      <span v-tooltip="formatDateTime(item.updatedAt)">{{ formatTimeAgo(item.updatedAt) }}</span>
    </div>
    
    <FileActionButtons
          :item="item"
          :can-paste="canPaste"
          :in-zip="inZip"
          @download-dir="downloadDir"
          @download-file="downloadFile"
          @upload-files="uploadFiles"
          @upload-dir="uploadDir"
          @delete-item="deleteItem"
          @duplicate-item="duplicateItem"
          @cut-item="cutItem"
          @copy-item="copyItem"
          @paste-item="pasteItem"
          @copy-link="copyLink"
          @rename-item="renameItem"
          @add-to-favorites="addToFavorites"
        />
  </section>

  <!-- Phone Layout -->
  <ListItemPhone
    v-else
    :is-selected="selectedIds.includes(item.id)"
    :is-selecting="shiftEffectingIds.includes(item.id)"
    :checkbox-checked="shiftEffectingIds.includes(item.id) ? shouldSelect : selectedIds.includes(item.id)"
    @click="handleItemClick($event, item, index, () => clickItem(item))"
    @mouseenter.stop="handleMouseOver($event, index)"
    @checkbox-click="(event: MouseEvent) => toggleSelect(event, item, index)"
  >
    <template #image>
      <div class="image" @click="viewItem($event, item)">
        <FileThumb
          :is-dir="item.isDir"
          :thumb-url="thumbUrl"
          :extension="item.extension"
          :thumb-error="imageErrorIds.includes(item.id)"
          :ext-error="extensionImageErrorIds.includes(item.id)"
          :on-thumb-error="() => onImageError(item.id)"
          :on-ext-error="() => onExtensionImageError(item.id)"
        />
      </div>
    </template>
    
    <template #title>
      {{ item.name }}
      <OnlinePreviewIcon :name="item.name" :is-dir="item.isDir" />
    </template>
    
    <template #subtitle>
      <span v-if="item.isDir">{{ $t('x_items', item.children || 0) }}</span>
      <span v-else>{{ formatFileSize(item.size) }}</span>
      <span v-tooltip="formatDateTime(item.updatedAt)">{{ formatTimeAgo(item.updatedAt) }}</span>
    </template>
    
    <template #actions>
      <FileActionButtons
        :item="item"
        :can-paste="canPaste"
        :in-zip="inZip"
        @download-dir="downloadDir"
        @download-file="downloadFile"
        @upload-files="uploadFiles"
        @upload-dir="uploadDir"
        @delete-item="deleteItem"
        @duplicate-item="duplicateItem"
        @cut-item="cutItem"
        @copy-item="copyItem"
        @paste-item="pasteItem"
        @copy-link="copyLink"
        @rename-item="renameItem"
        @add-to-favorites="addToFavorites"
      />
    </template>
  </ListItemPhone>
</template>

<script setup lang="ts">
import { fileThumbUrl, type IFile } from '@/lib/file'
import { computed } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'

// Extend IFile to include children property for directories
interface IFileWithChildren extends IFile {
  children?: number
}
import { formatFileSize, formatDateTime, formatTimeAgo } from '@/lib/format'
import OnlinePreviewIcon from '@/components/OnlinePreviewIcon.vue'

interface Props {
  item: IFileWithChildren
  index: number
  selectedIds: string[]
  shiftEffectingIds: string[]
  shouldSelect: boolean
  isPhone: boolean
  imageErrorIds: string[]
  extensionImageErrorIds: string[]
  canPaste: boolean
  inZip?: boolean
  // Functions passed from parent
  handleItemClick: (event: MouseEvent, item: IFile, index: number, callback: () => void) => void
  handleMouseOver: (event: MouseEvent, index: number) => void
  toggleSelect: (event: MouseEvent, item: IFile, index: number) => void
  onImageError: (id: string) => void
  onExtensionImageError: (id: string) => void
  viewItem: (event: Event, item: IFile) => void
  clickItem: (item: IFile) => void
}

const props = defineProps<Props>()

const { urlTokenKey } = storeToRefs(useTempStore())
const thumbUrl = computed(() => fileThumbUrl(urlTokenKey.value, props.item))

const emit = defineEmits<{
  downloadDir: [path: string]
  downloadFile: [path: string]
  uploadFiles: [path: string]
  uploadDir: [path: string]
  deleteItem: [item: IFile]
  duplicateItem: [item: IFile]
  cutItem: [item: IFile]
  copyItem: [item: IFile]
  pasteItem: [item: IFile]
  copyLink: [item: IFile]
  renameItem: [item: IFile]
  addToFavorites: [item: IFile]
}>()

function downloadDir(path: string) {
  emit('downloadDir', path)
}

function downloadFile(path: string) {
  emit('downloadFile', path)
}

function uploadFiles(path: string) {
  emit('uploadFiles', path)
}

function uploadDir(path: string) {
  emit('uploadDir', path)
}

function deleteItem(item: IFile) {
  emit('deleteItem', item)
}

function duplicateItem(item: IFile) {
  emit('duplicateItem', item)
}

function cutItem(item: IFile) {
  emit('cutItem', item)
}

function copyItem(item: IFile) {
  emit('copyItem', item)
}

function pasteItem(item: IFile) {
  emit('pasteItem', item)
}

function copyLink(item: IFile) {
  emit('copyLink', item)
}

function renameItem(item: IFile) {
  emit('renameItem', item)
}

function addToFavorites(item: IFile) {
  emit('addToFavorites', item)
}
</script>

<style scoped lang="scss">
.list-item-phone {
  margin-block-end: 8px;
}
</style> 