<template>
  <section
    v-if="!isPhone"
    class="file-item recent-file-item selectable-card"
    :class="{ selected: selectedIds.includes(item.id), selecting: shiftEffectingIds.includes(item.id) }"
    @click.stop="handleItemClick($event, item, index, () => clickItem(item))"
    @mouseenter.stop="handleMouseOver($event, index)"
  >
    <div class="start">
      <v-checkbox v-if="shiftEffectingIds.includes(item.id)" class="checkbox" touch-target="wrapper" :checked="shouldSelect" @click.stop="toggleSelect($event, item, index)" />
      <v-checkbox v-else class="checkbox" touch-target="wrapper" :checked="selectedIds.includes(item.id)" @click.stop="toggleSelect($event, item, index)" />
      <span class="number"><field-id :id="index + 1" :raw="item" /></span>
    </div>
    <div class="image" @click.stop="clickItem(item)">
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
    </div>
    <div class="subtitle">
      <span>{{ formatFileSize(item.size) }}</span>
      <span v-tooltip="formatDateTime(item.updatedAt)">{{ formatTimeAgo(item.updatedAt) }}</span>
    </div>
    <div class="actions">
      <v-icon-button v-tooltip="$t('download')" @click.stop="downloadFile(item.path)">
        <i-material-symbols:download-rounded />
      </v-icon-button>

      <v-dropdown v-model="infoOpen">
        <template #trigger>
          <v-icon-button v-tooltip="$t('info')">
            <i-material-symbols:info-outline-rounded />
          </v-icon-button>
        </template>
        <section class="card card-info">
          <div class="key-value vertical">
            <div class="key">{{ $t('path') }}</div>
            <div class="value">
              {{ item.path }}
            </div>
          </div>
        </section>
      </v-dropdown>
    </div>
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
      <div class="image" @click.stop="clickItem(item)">
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
    
    <template #title>{{ item.name }}</template>
    
    <template #subtitle>
      <span>{{ formatFileSize(item.size) }}</span>
      <span v-tooltip="formatDateTime(item.updatedAt)">{{ formatTimeAgo(item.updatedAt) }}</span>
    </template>
    
    <template #actions>
      <div class="actions">
        <v-icon-button v-tooltip="$t('download')" @click.stop="downloadFile(item.path)">
          <i-material-symbols:download-rounded />
        </v-icon-button>

      <v-dropdown v-model="infoOpenPhone">
        <template #trigger>
          <v-icon-button v-tooltip="$t('info')">
            <i-material-symbols:info-outline-rounded />
          </v-icon-button>
        </template>
        <section class="card card-info">
          <div class="key-value vertical">
            <div class="key">{{ $t('path') }}</div>
            <div class="value">
              {{ item.path }}
            </div>
          </div>
        </section>
      </v-dropdown>
      </div>
    </template>
  </ListItemPhone>
</template>

<script setup lang="ts">
import { fileThumbUrl, type IFile } from '@/lib/file'
import { computed, ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { formatFileSize, formatDateTime, formatTimeAgo } from '@/lib/format'

interface Props {
  item: IFile
  index: number
  selectedIds: string[]
  shiftEffectingIds: string[]
  shouldSelect: boolean
  isPhone: boolean
  imageErrorIds: string[]
  extensionImageErrorIds: string[]
  // Functions passed from parent
  handleItemClick: (event: MouseEvent, item: IFile, index: number, callback: () => void) => void
  handleMouseOver: (event: MouseEvent, index: number) => void
  toggleSelect: (event: MouseEvent, item: IFile, index: number) => void
  onImageError: (id: string) => void
  onExtensionImageError: (id: string) => void
  downloadFile: (path: string) => void
  clickItem: (item: IFile) => void
}

const props = defineProps<Props>()

const { urlTokenKey } = storeToRefs(useTempStore())
const thumbUrl = computed(() => fileThumbUrl(urlTokenKey.value, props.item))
const infoOpen = ref(false)
const infoOpenPhone = ref(false)
</script>

<style scoped lang="scss">
.list-item-phone {
  margin-block-end: 8px;
}
</style> 