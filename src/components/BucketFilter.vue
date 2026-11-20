<template>
  <div v-if="sortedBuckets.length" class="section-title">
    {{ $t('folders') }}
    <v-icon-button v-tooltip="isCollapsed ? $t('expand_all') : $t('collapse_all')" @click.prevent="toggleCollapsed">
      <i-material-symbols:expand-more-rounded v-if="isCollapsed" />
      <i-material-symbols:expand-less-rounded v-else />
    </v-icon-button>
  </div>
  <ul v-show="!isCollapsed" class="nav">
    <SidebarListItem
      v-for="item in sortedBuckets" :key="item.id" :title="item.name" :active="item.id === selected" :class="{ 'has-thumb': showThumb }"
      @click="view(mainStore, item.id)">
      <template v-if="showThumb" #start>
        <v-dropdown v-if="props.type === DataType.IMAGE" :model-value="rawBucketId === item.id" borderless @update:model-value="rawBucketId = $event ? item.id : null">
          <template #trigger>
            <button type="button" class="bucket-thumb-trigger" :aria-label="`${$t('session_management.view_raw')}: ${item.name}`" :aria-expanded="rawBucketId === item.id">
              <BucketThumb :items="item.topItemPaths" />
            </button>
          </template>
          <pre class="view-raw">{{ item }}</pre>
        </v-dropdown>
        <BucketThumb v-else :items="item.topItemPaths" />
      </template>
      <template #end>
        <span class="count">{{ item.itemCount.toLocaleString() }}</span>
      </template>
      <template #actions>
        <v-icon-button
          :id="'bucket-' + item.id" v-tooltip="$t('actions')" :aria-label="`${$t('actions')}: ${item.name}`" class="sm"
          @click.prevent.stop="showMenu(item)" @keydown.enter.stop @keydown.space.stop>
          <i-material-symbols:more-horiz />
        </v-icon-button>
      </template>
    </SidebarListItem>
  </ul>
  <v-dropdown-menu v-model="menuVisible" :anchor="'bucket-' + selectedItem?.id">
    <div class="dropdown-item" @click="renameFolder(selectedItem!); menuVisible = false">{{ $t('rename') }}</div>
  </v-dropdown-menu>
</template>

<script setup lang="ts">
import { AccessErrorCode } from '@/lib/api/access-error-code'
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
import { useRename } from '@/hooks/files'
import { directoryPathKey, normalizeDirectoryPath } from '@/lib/directory-picker-path'
import { fileNameError, getDirFromPath } from '@/lib/file'
import { openModal } from '@/components/modal'
import EditValueModal from '@/components/EditValueModal.vue'

const props = defineProps({
  type: { type: String as PropType<DataType>, required: true },
  selected: { type: String, required: true },
})

const { t } = useI18n()

const mainStore = useMainStore()
const mediaBuckets = ref<IBucket[]>([])
const rawBucketId = ref<string | null>(null)
const menuVisible = ref(false)
const selectedItem = ref<IBucket>()

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
    if (error === AccessErrorCode.NoPermission) {
      mediaBuckets.value = []
    } else if (error) {
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

function bucketDirectory(bucket: IBucket) {
  return getDirFromPath(normalizeDirectoryPath(bucket.topItemPaths[0]))
}

const { renameItem, renamedPath, renameMutation, renameVariables, renameDone } = useRename<{ id: string; path: string }>(async () => {
  const id = renameItem.value?.id
  const path = renamedPath.value!
  await refetch(undefined, true)
  if (props.selected !== id) return
  const renamed = mediaBuckets.value.find(bucket => directoryPathKey(bucketDirectory(bucket)) === directoryPathKey(path))
  if (renamed) view(mainStore, renamed.id)
})

function showMenu(item: IBucket) {
  selectedItem.value = item
  const anchorElement = document.getElementById('bucket-' + item.id)
  document.dispatchEvent(new CustomEvent('dropdown-toggle', { detail: { exclude: anchorElement } }))
  menuVisible.value = true
}

function renameFolder(item: IBucket) {
  renameItem.value = { id: item.id, path: bucketDirectory(item) }
  openModal(EditValueModal, {
    title: t('rename'), placeholder: t('name'), value: item.name, borderless: true,
    validate: fileNameError,
    mutation: renameMutation, getVariables: renameVariables, done: renameDone,
  })
}

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

<style scoped>
.bucket-thumb-trigger {
  display: block;
  padding: 0;
  border: 0;
  background: none;
  cursor: pointer;
}

.has-thumb :deep(.start) {
  width: 40px;
  height: 40px;
}
</style>
