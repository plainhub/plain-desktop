<template>
  <LightboxFileInfoItem v-if="sortedBuckets.length" :label="$t('move_to_folder')">
    <ul class="folder-grid">
      <li
        v-for="bucket in visibleBuckets"
        :key="bucket.id"
        class="folder-item"
        :class="{ current: isCurrentBucket(bucket) }"
        @click="onPick(bucket)"
      >
        <span class="thumb" :class="thumbClass(bucket)">
          <template v-if="bucketThumbs(bucket).length">
            <img v-for="(p, i) in bucketThumbs(bucket)" :key="i" class="thumb-img" :src="thumbUrl(p)" loading="lazy" alt="" onerror="this.style.display='none'" />
          </template>
          <i-material-symbols:folder-rounded v-else />
        </span>
        <span class="name">{{ bucket.name }}</span>
      </li>
    </ul>
    <button v-if="sortedBuckets.length > foldCount" class="toggle-more" @click.stop="showAll = !showAll">
      {{ showAll ? $t('collapse_all') : $t('expand_all') }}
    </button>
  </LightboxFileInfoItem>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { initQuery, mediaBucketsGQL } from '@/lib/api/query'
import type { IBucket, IMediaItemsActionedEvent } from '@/lib/interfaces'
import { useI18n } from 'vue-i18n'
import toast, { toastWithAction } from '@/components/toaster'
import emitter from '@/plugins/eventbus'
import { getDirFromPath, isZipPath } from '@/lib/file'
import { getFileId, getFileUrl } from '@/lib/api/file'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { useMoveItems } from '@/hooks/media'
import { moveMediaItemsGQL, initMutation } from '@/lib/api/mutation'
import { DataType } from '@/lib/data'
import { sortByName } from '@/lib/array'
import LightboxFileInfoItem from './LightboxFileInfoItem.vue'
import type { ISource } from './types'

const props = defineProps({
  current: {
    type: Object as () => ISource | undefined,
    required: true,
  },
})

const emit = defineEmits(['moved'])

const { t } = useI18n()
const tempStore = useTempStore()
const { urlTokenKey } = storeToRefs(tempStore)

const mediaBuckets = ref<IBucket[]>([])
const showAll = ref(false)
const foldCount = 8

const isTrashed = computed(() => props.current?.path?.includes('.trashed-') === true)
const inZip = computed(() => isZipPath(props.current?.path ?? ''))
const currentDir = computed(() => getDirFromPath(props.current?.path ?? ''))
const currentBucketId = computed(() => props.current?.data?.bucketId as string | undefined)

function isCurrentBucket(bucket: IBucket): boolean {
  if (currentBucketId.value) return bucket.id === currentBucketId.value
  return currentDir.value !== '' && bucket.topItems?.some((p) => getDirFromPath(p) === currentDir.value)
}

const sortedBuckets = computed(() =>
  sortByName(
    (mediaBuckets.value ?? []).filter((b) => !isCurrentBucket(b)),
    (b) => b.name ?? '',
    { numeric: true },
  ),
)

const visibleBuckets = computed(() =>
  showAll.value ? sortedBuckets.value : sortedBuckets.value.slice(0, foldCount),
)

function bucketThumbs(item: IBucket): string[] {
  return item.topItems?.slice(0, 4) ?? []
}

function thumbUrl(path: string): string {
  return getFileUrl(getFileId(urlTokenKey.value, path), '&w=128&h=128')
}

function thumbClass(item: IBucket) {
  return `count-${bucketThumbs(item).length}`
}

function onPick(bucket: IBucket) {
  const source = props.current
  if (!source?.data?.id || !source.type) return
  // topItems are paths inside the bucket, so the bucket dir can be derived from them
  const destDir = getDirFromPath(bucket.topItems?.[0] ?? '')
  if (!destDir) return
  const q = moveItems(source.type, [source.data.id], false, `ids:${source.data.id}`)
  if (q === undefined) return
  doMoveItems(destDir)
  toastWithAction(
    t('moved_to_folder', { folder: bucket.name }),
    t('undo'),
    () => undoMove(source.type as DataType, q, currentDir.value),
  )
}

const { moveLoading, moveItems, doMoveItems } = useMoveItems()

// moveMediaItems has no undo on the backend, so "undo" re-moves by ids back to the original dir.
function undoMove(type: DataType, query: string, origDir: string) {
  if (!origDir) return
  const { mutate } = initMutation({ document: moveMediaItemsGQL })
  mutate({ type, query, destDir: origDir })
}

const { refetch } = initQuery({
  handle: (data: { mediaBuckets: IBucket[] }, error: string) => {
    if (error) {
      toast(t(error), 'error')
    } else if (data) {
      mediaBuckets.value = data.mediaBuckets
    }
  },
  document: mediaBucketsGQL,
  variables: {
    type: DataType.IMAGE,
  },
})

const mediaItemsActionedHandler = (event: IMediaItemsActionedEvent) => {
  if (event.type === DataType.IMAGE) {
    refetch()
    if (event.action === 'move' && event.query === `ids:${props.current?.data?.id}`) {
      emit('moved')
    }
  }
}

onMounted(() => {
  emitter.on('media_items_actioned', mediaItemsActionedHandler)
})

onUnmounted(() => {
  emitter.off('media_items_actioned', mediaItemsActionedHandler)
})
</script>

<style lang="scss" scoped>
.folder-grid {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 8px;
}

.folder-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  cursor: pointer;
  padding: 6px 4px;
  border-radius: 8px;

  &:hover {
    background: color-mix(in srgb, var(--md-sys-color-primary) 10%, transparent);
  }

  .thumb {
    width: 44px;
    height: 44px;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 2px;
    border-radius: 10px;
    overflow: hidden;
    background: color-mix(in srgb, var(--md-sys-color-primary) 12%, transparent);

    &.count-1 {
      grid-template-columns: minmax(0, 1fr);
    }

    &.count-0 {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      font-size: 22px;
      color: var(--md-sys-color-on-surface-variant);
    }

    .thumb-img {
      width: 100%;
      height: 100%;
      min-width: 0;
      min-height: 0;
      display: block;
      object-fit: cover;
    }
  }

  .name {
    max-width: 100%;
    font-size: 0.75rem;
    color: var(--md-sys-color-on-surface-variant);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  &.current {
    opacity: 0.5;
    cursor: default;

    &:hover {
      background: none;
    }
  }
}

.toggle-more {
  margin-top: 8px;
  border: none;
  background: none;
  color: var(--md-sys-color-primary);
  cursor: pointer;
  padding: 4px 0;
  font-size: 0.85rem;

  &:hover {
    text-decoration: underline;
  }
}
</style>
