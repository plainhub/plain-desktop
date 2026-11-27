<template>
  <div class="top-app-bar">
    <span class="title" role="heading" aria-level="1" aria-live="polite">{{ $t('global_search_results', { count: total.toLocaleString() }) }}</span>
  </div>
  <div class="scroll-content">
    <NoDataPlaceholder v-if="!request" :loading="false" placeholder-key="global_search_enter_query" />
    <NoDataPlaceholder v-else-if="!targets.length" :loading="false" placeholder-key="global_search_select_devices" />
    <section v-for="group in groups" :key="group.device.id" class="search-group">
      <div v-if="targets.length > 1 || group.status !== 'ready'" class="top-app-bar" :class="{ 'search-device-loading': group.status === 'loading' }">
        <span v-if="targets.length > 1" class="title" role="heading" aria-level="2">{{ group.client?.name || group.device.name }}</span>
        <v-circular-progress v-if="group.status === 'loading'" indeterminate class="sm" />
        <span v-else-if="group.status === 'offline'" class="subtle">{{ $t('offline') }}</span>
        <span v-else-if="group.status === 'denied'" class="subtle">{{ $t('global_search_no_access') }}</span>
        <span v-else-if="group.status === 'FAILED'" class="subtle">{{ $t('global_search_failed') }}</span>
        <span v-else-if="targets.length > 1 && group.pages.every((page) => !page.loading && !page.failed)" class="subtle">{{ searchGroupTotal(group).toLocaleString() }}</span>
        <v-text-button v-if="group.status === 'FAILED' || group.status === 'offline'" @click="search.retry(group)">{{ $t('retry') }}</v-text-button>
      </div>
      <template v-if="group.status === 'ready'">
        <NoDataPlaceholder v-if="group.pages.every((page) => !page.loading && !page.failed && page.total === 0)" :loading="false" placeholder-key="search_no_results" />
        <template v-for="page in group.pages" :key="page.domain">
          <div v-if="page.hits.length || page.loading || page.failed || page.hits.length < page.total" class="search-group">
            <div v-if="(page.domain === 'images' || page.domain === 'videos') && page.hits.length" class="media-grid">
              <template v-for="hit in page.hits" :key="hit.item.id">
                <MediaGridItem
                  v-if="hit.domain === 'images' || hit.domain === 'videos'"
                  :data-type="hit.domain === 'images' ? DataType.IMAGE : DataType.VIDEO"
                  :checked="false" :selected="false" :shift-selected="false" :should-select="false" :selectable="false"
                  :aria-label="hit.item.title || getFileName(hit.item.path)"
                  @item-click="open(group, hit)" @view="open(group, hit)">
                  <template #thumbnail>
                    <img class="image image-thumb" :src="group.client?.thumbnailUrl(hit)" alt="" loading="lazy" @error="showBrokenThumbnail" />
                  </template>
                  <template #info-right>{{ hit.domain === 'videos' ? formatDurationMs(hit.item.durationMs) : formatFileSize(hit.item.size) }}</template>
                </MediaGridItem>
              </template>
            </div>
            <div v-else-if="page.domain === 'docs' && page.hits.length" class="main-list">
              <template v-for="(hit, index) in page.hits" :key="hit.item.id">
                <DocListItem v-if="hit.domain === 'docs'" :item="{ ...hit.item, tags: [] }" :index="index" read-only @open-file="open(group, hit)" />
              </template>
            </div>
            <template v-else>
              <template v-for="(hit, index) in page.hits" :key="hit.item.id">
                <Sidebar2ListItem
                  v-if="hit.domain === 'feeds' || hit.domain === 'notes'"
                  :item="hit.item" :index="index" :href="entryHref(group, hit)" :selectable="false"
                  @item-click="open(group, hit)"
                >
                  <template #title>{{ hit.item.title || $t(hit.domain === 'notes' ? 'meta_no_title' : 'no_content') }}</template>
                  <template #info>
                    <span v-if="hit.domain === 'feeds' && hit.item.author">{{ hit.item.author }}</span>
                    <span>{{ formatTimeAgo(hit.domain === 'feeds' ? hit.item.publishedAt : hit.item.updatedAt) }}</span>
                  </template>
                </Sidebar2ListItem>
              </template>
            </template>
            <div v-if="page.loading || page.failed || page.hits.length < page.total" class="secondary-actions" :class="{ 'search-page-loading': page.loading }">
              <v-circular-progress v-if="page.loading" indeterminate class="sm" />
              <template v-else-if="page.failed">
                <span class="subtle">{{ $t(`page_title.${page.domain}`) }} · {{ $t('global_search_failed') }}</span>
                <v-text-button @click="search.loadMore(group, page)">{{ $t('retry') }}</v-text-button>
              </template>
              <v-text-button v-else @click="search.loadMore(group, page)">
                {{ $t('load_more') }} · {{ $t(`page_title.${page.domain}`) }} ({{ page.hits.length }}/{{ page.total }})
              </v-text-button>
            </div>
          </div>
        </template>
      </template>
    </section>
  </div>
</template>

<script setup lang="ts">
import { inject } from 'vue'
import { useRouter } from 'vue-router'
import MediaGridItem from '@/components/media/MediaGridItem.vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import DocListItem from '@/views/docs/DocListItem.vue'
import Sidebar2ListItem from '@/components/Sidebar2ListItem.vue'
import { useOpenMedia } from '@/hooks/open-media'
import { globalSearchKey, searchGroupTotal, type SearchGroup } from '@/hooks/global-search'
import type { SearchHit } from '@/lib/api/global-search'
import { download, getFileName } from '@/lib/api/file'
import { FileAction } from '@/lib/api/file-action'
import { getTextFileId } from '@/lib/api/text-file'
import { isLocalMode } from '@/lib/device/local-mode'
import { openUrl } from '@/lib/browser'
import { openWindow } from '@/lib/api/tauri-window'
import { DataType } from '@/lib/data'
import { canOpenInBrowser, isOfficeFile, isTextFile } from '@/lib/file'
import { formatDurationMs, formatFileSize, formatTimeAgo } from '@/lib/format'

const { search } = inject(globalSearchKey)!
const { groups, targets, total, request } = search
const { open: openMedia } = useOpenMedia()
const router = useRouter()

function showBrokenThumbnail(event: Event) {
  const img = event.target as HTMLImageElement
  if (!img.src.endsWith('/broken-image.png')) img.src = '/broken-image.png'
}

function entryPath(hit: Extract<SearchHit, { domain: 'feeds' | 'notes' }>) {
  const id = encodeURIComponent(hit.item.id)
  return hit.domain === 'feeds' ? `/feeds/${encodeURIComponent(hit.item.feedId)}/entries/${id}` : `/notes/${id}`
}

function entryHref(group: SearchGroup, hit: Extract<SearchHit, { domain: 'feeds' | 'notes' }>) {
  const path = entryPath(hit)
  const query = !__IS_TAURI__ && group.device.target !== 'current' ? { __cid: group.device.id } : undefined
  return router.resolve({ path, query }).href
}

function open(group: SearchGroup, hit: SearchHit) {
  const client = group.client
  if (!client) return
  if (hit.domain === 'feeds' || hit.domain === 'notes') {
    const path = entryPath(hit)
    if (group.device.target === 'current') void router.push(path)
    else if (__IS_TAURI__) void openWindow(path, group.device.target === 'local' ? '' : group.device.id)
    else void openUrl(entryHref(group, hit))
    return
  }
  const name = getFileName(hit.item.path) || hit.item.title
  if (hit.domain === 'docs') {
    const textPreview = isTextFile(name) || isOfficeFile(name)
    if (group.device.target === 'managed' && textPreview) {
      const query = { peer: group.device.id, media: hit.item.id }
      void openUrl(router.resolve({ name: 'text-file', query }).href)
    } else if (textPreview && (group.device.target === 'current' || group.device.target === 'local')) {
      const local = group.device.target === 'local' || isLocalMode()
      const id = local ? getTextFileId(null, hit.item.path, true) : client.fileId(hit)
      const url = `/text-file?id=${encodeURIComponent(id)}`
      if (local) void openWindow(url, '')
      else void openUrl(url)
    } else if (canOpenInBrowser(name)) void openUrl(client.url(hit))
    else download(client.url(hit, FileAction.Download), name)
    return
  }
  const media = group.pages.flatMap((page) => page.hits).filter((item) => item.domain === 'images' || item.domain === 'videos')
  openMedia(media.indexOf(hit), media.map((entry) => ({
    src: client.url(entry),
    thumbnail: client.thumbnailUrl(entry),
    path: entry.item.path,
    name: getFileName(entry.item.path) || entry.item.title,
    size: entry.item.size,
    durationMs: entry.domain === 'videos' ? entry.item.durationMs : 0,
    updatedAt: entry.item.updatedAt,
  })), true)
}
</script>

<style scoped>
.search-group + .search-group {
  margin-block-start: 16px;
}

.search-device-loading {
  flex-wrap: nowrap;
}

.search-device-loading .title {
  flex: 0 1 auto;
  min-width: 0;
}

.search-device-loading > .sm {
  flex-shrink: 0;
}

.search-page-loading {
  justify-content: center;
}
</style>
