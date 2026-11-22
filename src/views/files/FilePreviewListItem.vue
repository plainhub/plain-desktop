<template>
  <FileItemLayout
    :class="{ 'recent-file-item': !$slots.actions }"
    :item="item" :index="index" :is-phone="isPhone" :selected="selected" :selecting="selecting"
    :checked="selecting ? list.selection.shouldSelect.value : selected"
    @click="list.selection.handleItemClick($event, item, index, () => list.openFile(item))"
    @mouseenter="list.selection.handleMouseOver($event, index)"
    @checkbox-click="list.selection.toggleSelect($event, item, index)"
  >
    <template #image>
      <div class="image" @click.stop="list.openFile(item)">
        <FileThumb
          :is-dir="item.isDir" :thumb-url="thumbUrl" :extension="item.extension"
          :thumb-error="thumbError" :ext-error="extError"
          :on-thumb-error="() => thumbError = true" :on-ext-error="() => extError = true"
        />
      </div>
    </template>
    <template #actions>
      <slot name="actions">
        <div class="actions">
          <v-icon-button v-tooltip="$t('download')" @click.stop="list.downloadFile(item.path)">
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
                <div class="value">{{ item.path }}</div>
              </div>
            </section>
          </v-dropdown>
        </div>
      </slot>
    </template>
  </FileItemLayout>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useTempStore } from '@/stores/temp'
import { fileThumbUrl, type IFile } from '@/lib/file'
import type { FilePreviewList } from '@/hooks/file-preview-list'
import FileItemLayout from './FileItemLayout.vue'

const props = defineProps<{ item: IFile; index: number; isPhone: boolean; list: FilePreviewList }>()
const temp = useTempStore()
const thumbUrl = computed(() => fileThumbUrl(temp.urlTokenKey, props.item))
const selected = computed(() => props.list.selection.selectedIdSet.value.has(props.item.id))
const selecting = computed(() => props.list.selection.shiftEffectingIdSet.value.has(props.item.id))
const thumbError = ref(false)
const extError = ref(false)
const infoOpen = ref(false)
</script>
