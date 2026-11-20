<template>
  <left-sidebar>
    <template #body>
      <ul class="nav">
        <SidebarListItem
          :title="$t('all')"
          :active="!selectedTagId && !type"
          @click="viewAll"
        >
          <template #start>
            <i-lucide:layout-grid />
          </template>
          <template v-if="counter.calls >= 0" #end>
            <span class="count">{{ counter.calls.toLocaleString() }}</span>
          </template>
        </SidebarListItem>
        <SidebarListItem
          v-for="t in callFilterTypes"
          :key="t"
          :title="$t(`call_type.${t}`)"
          :active="t === type"
          @click="openByType(t)"
        >
          <template #start>
            <i-material-symbols:call-received v-if="t === 'INCOMING'" />
            <i-material-symbols:call-made v-else-if="t === 'OUTGOING'" />
            <i-material-symbols:call-missed v-else />
          </template>
          <template v-if="getTypeCount(t) >= 0" #end>
            <span class="count">{{ getTypeCount(t).toLocaleString() }}</span>
          </template>
        </SidebarListItem>
      </ul>
      <tag-filter type="CALL" :selected="selectedTagId" />
    </template>
  </left-sidebar>
</template>

<script setup lang="ts">
import router, { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { useSearch } from '@/hooks/search'
import { callFilterTypes, type CallType, type IFilter } from '@/lib/interfaces'
import { decodeBase64, encodeBase64 } from '@/lib/strutil'
import { buildQuery } from '@/lib/search'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { callCountGQL, initLazyQuery } from '@/lib/api/query'
import emitter from '@/plugins/eventbus'

const mainStore = useMainStore()
const { counter } = storeToRefs(useTempStore())
const { parseQ } = useSearch()
const filter = reactive<IFilter>({
  tagIds: [],
})

const type = ref('')
const selectedTagId = ref('')
const typesCount = ref<Map<CallType, number>>(new Map())

const { fetch } = initLazyQuery({
  handle: (data: { total: number; incoming: number; outgoing: number; missed: number }) => {
    if (data) {
      counter.value.calls = data.total
      typesCount.value.set('INCOMING', data.incoming)
      typesCount.value.set('OUTGOING', data.outgoing)
      typesCount.value.set('MISSED', data.missed)
    }
  },
  document: callCountGQL,
  variables: () => ({}),
})

function getTypeCount(id: CallType) {
  return typesCount.value.get(id) ?? -1
}

function updateActive() {
  const route = router.currentRoute.value
  const q = decodeBase64(route.query.q?.toString() ?? '')
  parseQ(filter, q)
  type.value = filter.type ?? ''
  selectedTagId.value = filter.tagIds.length === 1 ? filter.tagIds[0] : ''
  if (type.value) {
    selectedTagId.value = ''
  }
  fetch()
}

updateActive()

watch(
  () => router.currentRoute.value,
  () => {
    updateActive()
  },
)

function openByType(type: CallType) {
  const q = buildQuery([
    {
      name: 'type',
      op: '',
      value: type,
    },
  ])
  replacePath(mainStore, `/calls?q=${encodeBase64(q)}`)
}

function viewAll() {
  replacePath(mainStore, '/calls')
}

const callsDeletedHandler = () => {
  fetch()
}

onMounted(() => {
  emitter.on('calls_deleted', callsDeletedHandler)
})

onUnmounted(() => {
  emitter.off('calls_deleted', callsDeletedHandler)
})
</script>
