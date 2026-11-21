<template>
  <div class="top-app-bar">
    <v-checkbox touch-target="wrapper" :checked="!!items.length && selected.length === items.length" :indeterminate="!!selected.length && selected.length < items.length" @change="selected = selected.length === items.length ? [] : items.map(item => item.id)" />
    <div class="title">{{ selected.length ? $t('x_selected', { count: selected.length }) : $t('trash') }}</div>
    <template v-if="selected.length">
      <v-icon-button :aria-label="$t('restore')" v-tooltip="$t('restore')" :disabled="busy" @click="apply(selected, false)"><i-material-symbols:restore-from-trash-outline-rounded /></v-icon-button>
      <v-icon-button id="files-trash-bulk" :aria-label="$t('permanent_delete')" v-tooltip="$t('permanent_delete')" :disabled="busy" @click="confirm(selected, 'files-trash-bulk')"><i-material-symbols:delete-forever-outline-rounded /></v-icon-button>
    </template>
  </div>
  <div class="scroller-wrapper">
    <div class="scroller main-list">
      <div v-for="(item, index) in items" :key="item.id" class="item">
        <v-checkbox touch-target="wrapper" :checked="selected.includes(item.id)" @change="toggle(item.id)" />
        <div class="title"><div>{{ item.displayName }}</div><small>{{ item.originalPath }}</small></div>
        <v-icon-button :aria-label="$t('restore')" v-tooltip="$t('restore')" :disabled="busy" @click="apply([item.id], false)"><i-material-symbols:restore-from-trash-outline-rounded /></v-icon-button>
        <v-icon-button :id="'files-trash-' + index" :aria-label="$t('permanent_delete')" v-tooltip="$t('permanent_delete')" :disabled="busy" @click="confirm([item.id], 'files-trash-' + index)"><i-material-symbols:delete-forever-outline-rounded /></v-icon-button>
      </div>
      <NoDataPlaceholder v-if="!items.length" :loading="loading" />
    </div>
  </div>
  <v-dropdown-menu v-model="confirming" :anchor="anchor">
    <inline-delete-confirm :count="pending.length" :loading="busy" @confirm="apply(pending, true)" @cancel="confirming = false" />
  </v-dropdown-menu>
</template>

<script setup lang="ts">
import { onActivated, onDeactivated, ref } from 'vue'
import { GqlError } from '@/lib/api/gql-error'
import { gqlFetch } from '@/lib/api/gql-client'
import { trashedFilesGQL } from '@/lib/api/query'
import { restoreFilesGQL, deleteTrashedFilesGQL } from '@/lib/api/mutation'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { showActionError } from '@/hooks/show-action-error'

interface TrashItem { id: string; displayName: string; originalPath: string }
const items = ref<TrashItem[]>([])
const selected = ref<string[]>([])
const pending = ref<string[]>([])
const anchor = ref('')
const confirming = ref(false)
const busy = ref(false)
const loading = ref(true)
let version = 0
let release: (() => void)[] = []

function toggle(id: string) {
  selected.value = selected.value.includes(id) ? selected.value.filter(value => value !== id) : [...selected.value, id]
}

function confirm(ids: string[], button: string) {
  pending.value = [...ids]
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
    const ids = new Set(rows.map(row => row.id))
    selected.value = selected.value.filter(id => ids.has(id))
  } catch (error) {
    if (request === version) showActionError(error)
  } finally {
    if (request === version) loading.value = false
  }
}

async function apply(ids: string[], purge: boolean) {
  if (busy.value) return
  confirming.value = false
  busy.value = true
  try {
    const response = await gqlFetch(purge ? deleteTrashedFilesGQL : restoreFilesGQL, { paths: ids }, { dedupe: false })
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
