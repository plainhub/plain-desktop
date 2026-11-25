<template>
  <Teleport v-if="isActive" to="#header-end-slot" defer>
    <v-icon-button v-tooltip="$t('refresh')" :loading="loading" @click="refetch">
      <i-material-symbols:refresh-rounded />
    </v-icon-button>
  </Teleport>
  <div class="scroll-content">
    <div v-if="loading" class="state-wrap">
      <v-circular-progress indeterminate />
    </div>
    <dev-data-table
      v-else-if="entries.length > 0"
      :columns="['key', 'value']"
      :rows="entries"
      :debug="app.debug || app.developerMode"
      :deleting-row="entries.find(row => row.key === deletingKey)"
      @delete="row => deleteEntry(String(row.key))"
    />
    <div v-else class="state-wrap">
      <i-lucide:archive class="state-icon" />
      <span class="state-text">{{ $t('no_data') }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { showActionError } from '@/hooks/show-action-error'
import { ref, onActivated, onDeactivated } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { initQuery, prefsGQL } from '@/lib/api/query'
import { initMutation, removePrefGQL } from '@/lib/api/mutation'
import DevDataTable from './DevDataTable.vue'


const isActive = ref(false)
onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })

const { app } = storeToRefs(useTempStore())

type KeyValuePair = {
  key: string
  value: string
}

const entries = ref<KeyValuePair[]>([])
const deletingKey = ref('')
const { loading, refetch } = initQuery({
  handle(data: { prefs: Record<string, unknown> }, error: string) {
    if (!error) {
      entries.value = toEntries(data?.prefs)
    }
  },
  document: prefsGQL,
})

const { mutate: deleteMutate } = initMutation({ document: removePrefGQL }, showActionError)

async function deleteEntry(key: string) {
  deletingKey.value = key
  await deleteMutate({ key })
  deletingKey.value = ''
  refetch()
}

function toEntries(prefs: Record<string, unknown> | undefined): KeyValuePair[] {
  return Object.entries(prefs ?? {})
    .map(([key, value]) => ({ key, value: JSON.stringify(value) }))
    .sort((a, b) => a.key.localeCompare(b.key))
}
</script>

<style lang="scss" scoped>
.scroll-content {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
}

.state-wrap {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  height: 200px;
  color: var(--md-sys-color-on-surface-variant);

  .state-icon {
    width: 40px;
    height: 40px;
    opacity: 0.5;
  }

  .state-text {
    font-size: 0.875rem;
  }
}
</style>

