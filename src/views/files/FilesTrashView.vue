<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="allChecked" :indeterminate="!allChecked && checked" @change="toggleAllChecked" />
    <div class="title">{{ selectedIds.length ? $t('x_selected', { count: selectedIds.length }) : $t('trash') }}</div>
    <template v-if="selectedIds.length">
      <v-icon-button v-tooltip="$t('restore')" :aria-label="$t('restore')" :disabled="busy" @click="apply(selectedIds, false)"><i-material-symbols:restore-from-trash-outline-rounded /></v-icon-button>
      <v-icon-button id="files-trash-bulk" v-tooltip="$t('delete')" :aria-label="$t('delete')" :disabled="busy" @click="confirm(selectedIds, 'files-trash-bulk', true)"><i-material-symbols:delete-forever-outline-rounded /></v-icon-button>
    </template>
  </div>
  <div class="scroller-wrapper">
    <div class="scroller main-list">
      <div v-if="items.length" role="list">
        <FilePreviewListItem
          v-for="(item, index) in items" :key="item.id" role="listitem"
          :item="item" :index="index" :is-phone="isPhone" :list="list"
        >
          <template #actions>
            <FileTrashActions :busy="busy" @restore="apply([item.path], false)" @delete="confirm([item.path], $event)" />
          </template>
        </FilePreviewListItem>
      </div>
      <NoDataPlaceholder v-else :loading="loading" />
    </div>
  </div>
  <delete-confirm-popup
    :model-value="pending !== null" :anchor="pending?.anchor ?? ''" :count="pending?.paths.length ?? 0" :loading="busy"
    @update:model-value="!$event && close()" @confirm="purgeConfirmed" />
</template>

<script setup lang="ts">
import { computed, onActivated, onDeactivated, inject, ref, type Ref } from 'vue'
import { GqlError } from '@/lib/api/gql-error'
import { gqlFetch } from '@/lib/api/gql-client'
import { trashedFilesGQL } from '@/lib/api/query'
import { restoreFilesGQL, deleteTrashedFilesGQL } from '@/lib/api/mutation'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { useListConfirmation } from '@/hooks/list'
import { showActionError } from '@/hooks/show-action-error'

import type { GqlResultType } from '@/lib/api/graphql/operations'
import { enrichFile, type IFile } from '@/lib/file'
import { useFilePreviewList } from '@/hooks/file-preview-list'
import FilePreviewListItem from './FilePreviewListItem.vue'
import FileTrashActions from './FileTrashActions.vue'

type TrashItem = GqlResultType<'trashedFilesGQL'>['trashedFiles'][number]
const isPhone = inject<Ref<boolean>>('isPhone')!
const items = ref<(IFile & Pick<TrashItem, 'childCount'>)[]>([])
const list = useFilePreviewList(items, true)
const { selectedIds, selectedIdSet, checked, allChecked, toggleAllChecked, clearSelection } = list.selection
const paths = computed(() => new Set(items.value.map((item) => item.path)))
const { pending, open, close } = useListConfirmation<{ paths: string[]; anchor: string; bulk: boolean }>((target) =>
  target.paths.every((path) => paths.value.has(path))
  && (!target.bulk || (target.paths.length === selectedIds.value.length && target.paths.every((path) => selectedIdSet.value.has(path)))),
)
const busy = ref(false)
const loading = ref(true)
let version = 0
let release: (() => void)[] = []

function confirm(paths: string[], anchor: string, bulk = false) {
  open({ paths: [...paths], anchor, bulk })
}

function purgeConfirmed() {
  if (pending.value) return apply(pending.value.paths, true)
}

async function refresh() {
  const request = ++version
  loading.value = true
  try {
    const rows: TrashItem[] = []
    while (request === version) {
      const response = await gqlFetch<{ trashedFiles: TrashItem[] }>(trashedFilesGQL, { offset: rows.length, limit: 200 }, { fresh: true })
      if (response.errors?.length) throw GqlError.fromGraphql(response.errors[0])
      const page = response.data!.trashedFiles
      rows.push(...page)
      if (page.length < 200) break
    }
    if (request !== version) return
    items.value = rows.map(enrichFile)
  } catch (error) {
    if (request === version) showActionError(error)
  } finally {
    if (request === version) loading.value = false
  }
}

async function apply(paths: string[], purge: boolean) {
  if (busy.value) return
  close()
  busy.value = true
  try {
    const response = await gqlFetch(purge ? deleteTrashedFilesGQL : restoreFilesGQL, { paths }, { dedupe: false })
    if (response.errors?.length) throw GqlError.fromGraphql(response.errors[0])
  } catch (error) { showActionError(error) }
  finally { busy.value = false }
}

onActivated(() => {
  release = [
    graphqlSubscriptions.subscribe('contentChanged', refresh),
    graphqlSubscriptions.subscribe('mediaUpdated', refresh),
    graphqlSubscriptions.onAttached(refresh),
  ]
  void refresh()
})
onDeactivated(() => {
  version++
  release.forEach(stop => stop())
  items.value = []
  clearSelection()
  close()
})
</script>
