<template>
  <LightboxFileInfoItem v-if="sortedBuckets.length" :label="$t('move_to_folder')">
    <ul class="folder-grid">
      <li v-for="bucket in visibleBuckets" :key="bucket.id" class="folder-item" @click="onPick(bucket)">
        <BucketThumb :items="bucket.topItemPaths" />
        <span class="name">{{ bucket.name }}</span>
      </li>
    </ul>
    <a v-if="sortedBuckets.length > foldCount" href="#" class="show-more" @click.prevent="showAll = !showAll">
      {{ showAll ? $t('show_less') : $t('show_more') }}
    </a>
  </LightboxFileInfoItem>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { initQuery, mediaBucketsGQL } from '@/lib/api/query'
import type { IBucket, IMediaItemsActionedEvent } from '@/lib/interfaces'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import emitter from '@/plugins/eventbus'
import { getDirFromPath, isZipPath } from '@/lib/file'
import { useMoveItems } from '@/hooks/media'
import { useOrganizeUndo } from '@/hooks/organize-undo'
import { DataType } from '@/lib/data'
import { sortByName } from '@/lib/array'
import BucketThumb from '@/components/BucketThumb.vue'
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

const mediaBuckets = ref<IBucket[]>([])
const showAll = ref(false)
const foldCount = 8

const isTrashed = computed(() => props.current?.path?.includes('.trashed-') === true)
const inZip = computed(() => isZipPath(props.current?.path ?? ''))
const currentDir = computed(() => getDirFromPath(props.current?.path ?? ''))
const currentBucketId = computed(() => props.current?.data?.bucketId as string | undefined)

function isCurrentBucket(bucket: IBucket): boolean {
  if (currentBucketId.value) return bucket.id === currentBucketId.value
  return currentDir.value !== '' && bucket.topItemPaths?.some((p) => getDirFromPath(p) === currentDir.value)
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

function onPick(bucket: IBucket) {
  const source = props.current
  if (!source?.data?.id || !source.type) return
  // topItemPaths are paths inside the bucket, so the bucket dir can be derived from them
  const destDir = getDirFromPath(bucket.topItemPaths?.[0] ?? '')
  if (!destDir) return
  const q = moveItems(source.type, [source.data.id], false, `ids:${source.data.id}`)
  if (q === undefined) return
  doMoveItems(destDir)
  record({ kind: 'move', type: source.type as DataType, query: q, origDir: currentDir.value })
}

const { moveLoading, moveItems, doMoveItems } = useMoveItems()
const { record } = useOrganizeUndo()

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
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
}

.folder-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 4px;
  border-radius: 8px;
  cursor: pointer;

  &:hover {
    background: color-mix(in srgb, var(--md-sys-color-primary) 10%, transparent);
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

.show-more {
  display: inline-block;
  margin-top: 8px;
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--md-sys-color-primary);
  text-decoration: none;

  &:hover {
    text-decoration: underline;
  }
}
</style>
