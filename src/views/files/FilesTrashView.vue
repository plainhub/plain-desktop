<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="!!items.length && selected.length === items.length" :indeterminate="!!selected.length && selected.length < items.length" @change="selected = selected.length === items.length ? [] : items.map(item => item.path)" />
    <div class="title">{{ selected.length ? $t('x_selected', { count: selected.length }) : $t('trash') }}</div>
    <template v-if="selected.length">
      <v-icon-button :aria-label="$t('restore')" v-tooltip="$t('restore')" :disabled="busy" @click="apply(selected, false)"><i-material-symbols:restore-from-trash-outline-rounded /></v-icon-button>
      <v-icon-button id="files-trash-bulk" :aria-label="$t('delete')" v-tooltip="$t('delete')" :disabled="busy" @click="confirm(selected, 'files-trash-bulk')"><i-material-symbols:delete-forever-outline-rounded /></v-icon-button>
    </template>
  </div>
  <div class="scroller-wrapper">
    <div class="scroller main-list">
      <div v-if="items.length" role="list">
        <FileTrashItem
          v-for="(item, index) in items" :key="item.path" role="listitem"
          :item="item" :index="index" :selected="selected.includes(item.path)" :is-phone="isPhone" :busy="busy"
          @toggle="toggle(item.path)" @restore="apply([item.path], false)" @delete="confirm([item.path], $event)"
        />
      </div>
      <NoDataPlaceholder v-else :loading="loading" />
    </div>
  </div>
  <v-dropdown-menu v-model="confirming" :anchor="anchor">
    <inline-delete-confirm :count="pending.length" :loading="busy" @confirm="apply(pending, true)" @cancel="confirming = false" />
  </v-dropdown-menu>
</template>

<script setup lang="ts">
import { onActivated, onDeactivated, inject, ref, type Ref } from 'vue'
import { GqlError } from '@/lib/api/gql-error'
import { gqlFetch } from '@/lib/api/gql-client'
import { trashedFilesGQL } from '@/lib/api/query'
import { restoreFilesGQL, deleteTrashedFilesGQL } from '@/lib/api/mutation'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { showActionError } from '@/hooks/show-action-error'

import type { GqlResultType } from '@/lib/api/graphql/operations'
import FileTrashItem from './FileTrashItem.vue'

type TrashItem = GqlResultType<'trashedFilesGQL'>['trashedFiles'][number]
const isPhone = inject<Ref<boolean>>('isPhone')!
const items = ref<TrashItem[]>([])
const selected = ref<string[]>([])
const pending = ref<string[]>([])
const anchor = ref('')
const confirming = ref(false)
const busy = ref(false)
const loading = ref(true)
let version = 0
let release: (() => void)[] = []

function toggle(path: string) {
  selected.value = selected.value.includes(path) ? selected.value.filter(value => value !== path) : [...selected.value, path]
}

function confirm(paths: string[], button: string) {
  pending.value = [...paths]
  anchor.value = button
  confirming.value = true
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
    items.value = rows
    const paths = new Set(rows.map(row => row.path))
    selected.value = selected.value.filter(path => paths.has(path))
  } catch (error) {
    if (request === version) showActionError(error)
  } finally {
    if (request === version) loading.value = false
  }
}

async function apply(paths: string[], purge: boolean) {
  if (busy.value) return
  confirming.value = false
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
  selected.value = []
  confirming.value = false
})
</script>
