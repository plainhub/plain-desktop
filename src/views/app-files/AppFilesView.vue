<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="allChecked" :indeterminate="!allChecked && checked" @change="toggleAllChecked" />
    <div class="title">
      <span v-if="checked">{{ $t('x_selected', { count: realAllChecked ? total.toLocaleString() : selectedIds.length.toLocaleString() }) }}</span>
      <span v-else>{{ $t('app_files') }} ({{ total }})</span>
      <bulk-download-button v-if="checked" :single="realAllChecked || selectedIds.length === 1" @download="downloadSelected" @download-each="downloadSelectedEach" @download-zip="downloadItemsZip(realAllChecked, selectedIds, '')" />
    </div>
    <v-icon-button v-if="!checked" v-tooltip="$t('refresh')" @click="refresh">
      <i-material-symbols:refresh-rounded />
    </v-icon-button>
  </div>

  <div v-if="loading && firstInit" class="scroller-wrapper app-files-list">
    <div class="scroller main-list">
      <FileSkeletonItem v-for="i in 10" :key="i" :index="i" :is-phone="isPhone" />
    </div>
  </div>
  <NoDataPlaceholder v-if="items.length === 0 && !loading" :loading="loading" />
  <div v-else class="scroller-wrapper app-files-list">
    <div class="scroller main-list" :class="{ 'select-mode': checked }">
      <AppFileItem
        v-for="(item, index) in items"
        :key="item.id"
        :item="item"
        :index="index"
        :selected-ids="selectedIds"
        :shift-effecting-ids="shiftEffectingIds"
        :should-select="shouldSelect"
        :handle-item-click="handleItemClick"
        :handle-mouse-over="handleMouseOver"
        :toggle-select="toggleSelect"
        @click="clickItem(item)"
      />
      <div ref="sentinel" class="scroll-sentinel"></div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { inject, ref, watch, onActivated, onDeactivated } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { getFileUrl, getFileId, download } from '@/lib/api/file'
import { getApiBaseUrl } from '@/lib/api/api'
import { isTextFile, canOpenInBrowser, canView } from '@/lib/file'
import { openUrl } from '@/lib/browser'
import { noDataKey } from '@/lib/list'
import { useSelectable } from '@/hooks/list'
import { useDownload, useDownloadItems } from '@/hooks/files'
import type { ISource } from '@/components/lightbox/types'
import FileSkeletonItem from '@/views/files/FileSkeletonItem.vue'
import { useAppFilesData, getAppFileFid, type IAppFile } from './useAppFilesData'
import AppFileItem from './AppFileItem.vue'
import { useOpenMedia } from '@/hooks/open-media'

const isPhone = inject('isPhone') as boolean
const { urlTokenKey } = storeToRefs(useTempStore())
const { open: openMedia } = useOpenMedia()

const { items, total, loading, firstInit, fetch, loadMore, refresh } = useAppFilesData()

const sel = useSelectable(items)
const { selectedIds, allChecked, realAllChecked, checked, toggleAllChecked, toggleSelect, shiftEffectingIds, handleItemClick, handleMouseOver, shouldSelect, clearSelection } = sel
sel.total = total

const { downloadFile } = useDownload(urlTokenKey)
const { downloadItems: downloadItemsZip } = useDownloadItems(urlTokenKey, 'APP_FILE', clearSelection, () => 'app-files.zip')

function downloadSelected() {
  if (!realAllChecked.value && selectedIds.value.length === 1) {
    const item = items.value.find((it) => it.id === selectedIds.value[0])
    if (item) {
      downloadFile(`fid:${getAppFileFid(item)}`, item.fileName)
      clearSelection()
      return
    }
  }
  downloadItemsZip(realAllChecked.value, selectedIds.value, '')
}

async function downloadSelectedEach() {
  const selected = items.value.filter((it: IAppFile) => selectedIds.value.includes(it.id))
  for (const item of selected) {
    downloadFile(`fid:${getAppFileFid(item)}`, item.fileName)
    await new Promise((resolve) => setTimeout(resolve, 250))
  }
  clearSelection()
}

const sentinel = ref<HTMLElement | null>(null)
let observer: IntersectionObserver | null = null

function setupObserver() {
  if (observer) { observer.disconnect(); observer = null }
  if (!sentinel.value) return
  observer = new IntersectionObserver(
    (entries) => { if (entries[0]?.isIntersecting) loadMore() },
    { rootMargin: '200px' },
  )
  observer.observe(sentinel.value)
}

watch(sentinel, (el) => { if (el) setupObserver() })

function getAppFileUrl(item: IAppFile) {
  return getFileUrl(getFileId(urlTokenKey.value, `fid:${getAppFileFid(item)}`))
}

function getAppFileDownloadUrl(item: IAppFile) {
  const path = `fid:${getAppFileFid(item)}`
  const fileId = getFileId(urlTokenKey.value, JSON.stringify({ path, name: item.fileName }))
  return `${getApiBaseUrl()}/fs?id=${encodeURIComponent(fileId)}&dl=1`
}

function clickItem(item: IAppFile) {
  const name = item.fileName
  if (isTextFile(name)) {
    const fileId = getFileId(urlTokenKey.value, `fid:${getAppFileFid(item)}`)
    openUrl(`/text-file?id=${encodeURIComponent(fileId)}`)
  } else if (canOpenInBrowser(name)) {
    openUrl(getAppFileUrl(item))
  } else if (canView(name)) {
    viewMedia(item)
  } else {
    download(getAppFileDownloadUrl(item), name)
  }
}

function viewMedia(item: IAppFile) {
  const sources = items.value
    .filter((it) => canView(it.fileName))
    .map((it) => ({
      path: `fid:${getAppFileFid(it)}`, src: getAppFileUrl(it), name: it.fileName, size: it.size, durationMs: 0,
    })) as ISource[]
  const targetPath = `fid:${getAppFileFid(item)}`
  const index = sources.findIndex((s) => s.path === targetPath)
  if (index < 0) {
    openMedia(0, [item] as unknown as ISource[])
    return
  }
  openMedia(index, sources)
}

const isActive = ref(false)
onActivated(() => {
  isActive.value = true
  fetch()
})
onDeactivated(() => {
  isActive.value = false
  observer?.disconnect()
  observer = null
})
</script>

<style lang="scss" scoped>
.scroll-sentinel {
  height: 1px;
}
</style>

<style lang="scss">
.app-files-list .file-item {
  grid-template-areas: 'start image title actions' 'start image subtitle actions';
  grid-template-columns: 48px 50px auto 80px;
  cursor: pointer;
  .image {
    width: 50px; height: 50px; grid-area: image; margin-block: 12px 8px; text-align: center;
    .svg { max-width: 50px; max-height: 50px; }
  }
  .title { margin-inline: 16px; padding-block-start: 8px; user-select: none; }
  .subtitle { grid-area: subtitle; display: flex; gap: 8px; font-size: 0.875rem; margin-inline: 16px; margin-block: 8px; user-select: none; }
  .actions { justify-content: end; }
}
</style>
