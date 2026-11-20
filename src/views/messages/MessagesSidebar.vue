<template>
  <left-sidebar>
    <template #body>
      <ul class="nav">
        <SidebarListItem
          :title="$t('all')"
          :active="!selectedTagId && !type && !isArchived"
          @click="viewAll"
        >
          <template #start>
            <i-lucide:layout-grid />
          </template>
          <template v-if="counter.messages >= 0" #end>
            <span class="count">{{ counter.messages.toLocaleString() }}</span>
          </template>
        </SidebarListItem>
        <SidebarListItem
          v-for="t in smsFilterTypes"
          :key="t"
          :title="$t(`message_type.${t}`)"
          :active="t === type"
          @click="openByType(t)"
        >
          <template #start>
            <i-material-symbols:inbox-outline-rounded v-if="t === 'INBOX'" />
            <i-material-symbols:send-outline-rounded v-else-if="t === 'SENT'" />
            <i-material-symbols:draft-outline-rounded v-else />
          </template>
          <template v-if="getTypeCount(t) >= 0" #end>
            <span class="count">{{ getTypeCount(t).toLocaleString() }}</span>
          </template>
        </SidebarListItem>
        <SidebarListItem
          :title="$t('archived')"
          :active="isArchived"
          @click="viewArchived"
        >
          <template #start>
            <i-material-symbols:archive-outline-rounded />
          </template>
        </SidebarListItem>
      </ul>
      <tag-filter type="SMS" :selected="selectedTagId" />
    </template>
  </left-sidebar>
</template>

<script setup lang="ts">
import router, { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { reactive, ref, watch } from 'vue'
import { useSearch } from '@/hooks/search'
import { smsFilterTypes, type SmsType, type IFilter } from '@/lib/interfaces'
import { decodeBase64, encodeBase64 } from '@/lib/strutil'
import { buildQuery } from '@/lib/search'
import { useTempStore } from '@/stores/temp'
import { useSmsStore } from '@/stores/sms'
import { storeToRefs } from 'pinia'
import SidebarListItem from '@/components/SidebarListItem.vue'

const mainStore = useMainStore()
const { counter } = storeToRefs(useTempStore())
const smsStore = useSmsStore()
const { typesCount } = storeToRefs(smsStore)
const { parseQ } = useSearch()
const filter = reactive<IFilter>({
  tagIds: [],
})
const type = ref('')
const isArchived = ref(false)
const selectedTagId = ref('')

function getTypeCount(id: SmsType) {
  return typesCount.value.get(id) ?? -1
}

function updateActive() {
  const route = router.currentRoute.value
  const q = decodeBase64(route.query.q?.toString() ?? '')
  parseQ(filter, q)
  type.value = filter.type ?? ''
  selectedTagId.value = filter.tagIds.length === 1 ? filter.tagIds[0] : ''
  isArchived.value = router.currentRoute.value.path.startsWith('/messages/archived')
  if (type.value) {
    selectedTagId.value = ''
  }
  smsStore.fetchCounts()
}

updateActive()

watch(
  () => router.currentRoute.value,
  () => {
    updateActive()
  }
)

function openByType(type: SmsType) {
  const q = buildQuery([
    {
      name: 'type',
      op: '',
      value: type,
    },
  ])
  replacePath(mainStore, `/messages?q=${encodeBase64(q)}`)
}

function viewAll() {
  replacePath(mainStore, '/messages')
}

function viewArchived() {
  replacePath(mainStore, '/messages/archived')
}
</script>
