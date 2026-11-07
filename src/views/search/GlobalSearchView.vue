<template>
  <div class="top-app-bar">
    <span class="title" role="heading" aria-level="1" aria-live="polite">{{ $t('global_search_results', { count: total.toLocaleString() }) }}</span>
  </div>
  <div class="scroll-content">
    <NoDataPlaceholder v-if="!request" :loading="false" placeholder-key="global_search_enter_query" />
    <NoDataPlaceholder v-else-if="!targets.length" :loading="false" placeholder-key="global_search_select_devices" />
    <section v-for="group in groups" :key="group.device.id">
      <div v-if="targets.length > 1 || group.status !== 'ready'" class="top-app-bar">
        <span v-if="targets.length > 1" class="title" role="heading" aria-level="2">{{ group.client?.name || group.device.name }}</span>
        <v-circular-progress v-if="group.status === 'loading'" indeterminate class="sm" />
        <span v-else-if="group.status === 'offline'" class="subtle">{{ $t('offline') }}</span>
        <span v-else-if="group.status === 'denied'" class="subtle">{{ $t('global_search_no_access') }}</span>
        <span v-else-if="group.status === 'failed'" class="subtle">{{ $t('global_search_failed') }}</span>
        <span v-else-if="targets.length > 1 && group.pages.every((page) => !page.loading && !page.failed)" class="subtle">{{ searchGroupTotal(group).toLocaleString() }}</span>
        <v-text-button v-if="group.status === 'failed' || group.status === 'offline'" @click="search.retry(group)">{{ $t('retry') }}</v-text-button>
      </div>
      <template v-if="group.status === 'ready'">
        <NoDataPlaceholder v-if="group.pages.every((page) => !page.loading && !page.failed && page.total === 0)" :loading="false" placeholder-key="search_no_results" />
        <template v-for="page in group.pages" :key="page.domain">
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
          <ul v-else-if="page.hits.length" class="list-items">
            <VListItem
              v-for="hit in page.hits" :key="hit.item.id"
              :title="hit.item.title || ('path' in hit.item ? getFileName(hit.item.path) : $t('no_content'))"
              :subtitle="'path' in hit.item ? hit.item.path : hit.domain === 'feeds' ? hit.item.author : undefined"
              :value="'size' in hit.item ? formatFileSize(hit.item.size) : formatTimeAgo(hit.item.updatedAt)"
              clickable tabindex="0" @click="open(group, hit)" @keydown.enter="open(group, hit)">
              <template #start>
                <i-lucide:rss v-if="hit.domain === 'feeds'" />
                <i-lucide:notebook-pen v-else-if="hit.domain === 'notes'" />
                <i-lucide:file-text v-else />
              </template>
            </VListItem>
          </ul>
          <div v-if="page.loading || page.failed || page.hits.length < page.total" class="secondary-actions">
            <v-circular-progress v-if="page.loading" indeterminate class="sm" />
            <template v-else-if="page.failed">
              <span class="subtle">{{ $t(`page_title.${page.domain}`) }} · {{ $t('global_search_failed') }}</span>
              <v-text-button @click="search.loadMore(group, page)">{{ $t('retry') }}</v-text-button>
            </template>
            <v-text-button v-else @click="search.loadMore(group, page)">
              {{ $t('load_more') }} · {{ $t(`page_title.${page.domain}`) }} ({{ page.hits.length }}/{{ page.total }})
            </v-text-button>
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
import VListItem from '@/components/base/VListItem.vue'
import { useOpenMedia } from '@/hooks/open-media'
import { globalSearchKey, searchGroupTotal, type SearchGroup } from '@/hooks/global-search'
import type { SearchHit } from '@/lib/api/global-search'
import { download, getFileName } from '@/lib/api/file'
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

function open(group: SearchGroup, hit: SearchHit) {
  const client = group.client
  if (!client) return
  if (hit.domain === 'feeds' || hit.domain === 'notes') {
    const id = encodeURIComponent(hit.item.id)
    const path = hit.domain === 'feeds' ? `/feeds/${encodeURIComponent(hit.item.feedId)}/entries/${id}` : `/notes/${id}`
    if (group.device.target === 'current') void router.push(path)
    else if (__IS_TAURI__) void openWindow(path, group.device.target === 'local' ? '' : group.device.id)
    else void openUrl(router.resolve({ path, query: { __cid: group.device.id } }).href)
    return
  }
  const name = getFileName(hit.item.path) || hit.item.title
  if (hit.domain === 'docs') {
    if (group.device.target === 'current' && (isTextFile(name) || isOfficeFile(name))) {
      void openUrl(`/text-file?id=${encodeURIComponent(client.fileId(hit))}`)
    } else if (canOpenInBrowser(name)) void openUrl(client.url(hit))
    else download(client.url(hit), name)
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
