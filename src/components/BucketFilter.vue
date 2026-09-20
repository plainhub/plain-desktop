<template>
  <div v-if="sortedBuckets.length" class="section-title">
    {{ $t('folders') }}
    <v-icon-button v-tooltip="isCollapsed ? $t('expand_all') : $t('collapse_all')" @click.prevent="toggleCollapsed">
      <i-material-symbols:expand-more-rounded v-if="isCollapsed" />
      <i-material-symbols:expand-less-rounded v-else />
    </v-icon-button>
  </div>
  <ul v-show="!isCollapsed" class="nav">
    <li
      v-for="item in sortedBuckets" :key="item.id" :class="{ active: selected && item.id === selected }"
      @click.prevent="view(mainStore, item.id)">
      <BucketThumb v-if="showThumb" :items="item.topItemPaths" />
      <span class="title">{{ item.name }}</span><span class="count">{{ item.itemCount.toLocaleString() }}</span>
    </li>
  </ul>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, type PropType } from 'vue'
import { initQuery, mediaBucketsGQL } from '@/lib/api/query'
import type { IBucket, IMediaItemsActionedEvent } from '@/lib/interfaces'
import { useMainStore } from '@/stores/main'
import toast from '@/components/toaster'
import { useI18n } from 'vue-i18n'
import emitter from '@/plugins/eventbus'
import { useBuckets } from '@/hooks/media'
import { DataType } from '@/lib/data'
import { sortByName } from '@/lib/array'
import BucketThumb from '@/components/BucketThumb.vue'

const props = defineProps({
  type: { type: String as PropType<DataType>, required: true },
  selected: { type: String, required: true },
})

const { t } = useI18n()

const mainStore = useMainStore()
const mediaBuckets = ref<IBucket[]>([])

const isCollapsed = computed(() => !!mainStore.bucketFilterCollapsed?.[props.type])

const showThumb = computed(() => props.type === DataType.IMAGE || props.type === DataType.VIDEO)

function toggleCollapsed() {
  mainStore.bucketFilterCollapsed[props.type] = !isCollapsed.value
}

const sortedBuckets = computed(() =>
  sortByName(mediaBuckets.value ?? [], (b) => b.name ?? '', { numeric: true })
)
const { view } = useBuckets(props.type)

const { refetch } = initQuery({
  handle: (data: { mediaBuckets: IBucket[] }, error: string) => {
    if (error) {
      toast(t(error), 'error')
    } else {
      if (data) {
        mediaBuckets.value = data.mediaBuckets
      }
    }
  },
  document: mediaBucketsGQL,
  variables: {
    type: props.type,
  },
})

const mediaItemsActionedHandler = (event: IMediaItemsActionedEvent) => {
  if (event.type === props.type) {
    refetch()
  }
}

onMounted(() => {
  emitter.on('media_items_actioned', mediaItemsActionedHandler)
})

onUnmounted(() => {
  emitter.off('media_items_actioned', mediaItemsActionedHandler)
})
</script>
