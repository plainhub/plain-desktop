<template>
  <Teleport v-if="isActive" to="#header-end-slot" defer>
    <v-icon-button v-tooltip="$t('refresh')" :loading="loading" @click="refetch">
      <i-material-symbols:refresh-rounded />
    </v-icon-button>
  </Teleport>
  <div class="scroll-content">
    <div class="button-group tab-group" role="tablist" aria-label="Preferences">
      <button
        type="button"
        role="tab"
        :aria-selected="activeTab === 'system'"
        :class="{ selected: activeTab === 'system' }"
        @click="activeTab = 'system'"
      >
        {{ $t('device.system') }}
      </button>
      <button
        type="button"
        role="tab"
        :aria-selected="activeTab === 'user'"
        :class="{ selected: activeTab === 'user' }"
        @click="activeTab = 'user'"
      >
        {{ $t('user') }}
      </button>
    </div>
    <div v-if="loading" class="state-wrap">
      <v-circular-progress indeterminate />
    </div>
    <dev-data-table
      v-else-if="entries.length > 0"
      :columns="['key', 'value']"
      :rows="entries"
      row-key="key"
      :debug="app.debug && activeTab === 'user'"
      :deleting-key="deletingKey"
      @delete="deleteEntry"
    />
    <div v-else class="state-wrap">
      <i-lucide:archive class="state-icon" />
      <span class="state-text">{{ $t('no_data') }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onActivated, onDeactivated } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { initQuery, prefsGQL } from '@/lib/api/query'
import { initMutation, removeUserPrefGQL } from '@/lib/api/mutation'
import DevDataTable from './DevDataTable.vue'

const isActive = ref(false)
onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })

const { app } = storeToRefs(useTempStore())
const activeTab = ref<'system' | 'user'>('user')

interface KeyValuePair {
  key: string
  value: string
}

const systemEntries = ref<KeyValuePair[]>([])
const userEntries = ref<KeyValuePair[]>([])
const entries = computed(() => activeTab.value === 'system' ? systemEntries.value : userEntries.value)
const deletingKey = ref('')
const { loading, refetch } = initQuery({
  handle(data: { systemPrefs: Record<string, unknown>; userPrefs: Record<string, unknown> }, error: string) {
    if (!error) {
      systemEntries.value = toEntries(data?.systemPrefs)
      userEntries.value = toEntries(data?.userPrefs)
    }
  },
  document: prefsGQL,
})

const { mutate: deleteMutate } = initMutation({ document: removeUserPrefGQL })

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

.tab-group {
  width: fit-content;
  margin-bottom: 16px;
  background: var(--md-sys-color-surface-container);
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

