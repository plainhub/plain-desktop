<template>
  <Teleport v-if="isActive" to="#header-end-slot" defer>
    <v-dropdown v-if="dbPath" v-model="pathOpen">
      <template #trigger>
        <v-icon-button>
          <i-lucide:info />
        </v-icon-button>
      </template>
      <section class="card card-info">
        <div class="key-value vertical">
          <div class="key">{{ $t('path') }}</div>
          <div class="value">{{ dbPath }}</div>
        </div>
      </section>
    </v-dropdown>
    <v-icon-button v-tooltip="$t('refresh')" :loading="tablesLoading || rowsLoading" @click="refreshData">
      <i-material-symbols:refresh-rounded />
    </v-icon-button>
  </Teleport>
  <div v-if="tablesLoading && tables.length === 0" class="state-wrap">
    <v-circular-progress indeterminate />
  </div>
  <template v-else-if="tables.length > 0">
    <div class="tabs-wrap">
      <v-chip-set>
        <v-filter-chip
          v-for="t in tables"
          :key="t"
          :label="t"
          :selected="activeTable === t"
          @click="selectTable(t)"
        />
      </v-chip-set>
    </div>

    <div class="scroll-content">
      <div v-if="(rowsLoading || columnsLoading) && columns.length === 0" class="state-wrap">
        <v-circular-progress indeterminate />
      </div>
      <template v-else-if="columns.length > 0">
        <dev-data-table
          :columns="columns"
          :rows="rows"
          :row-key="idKey"
          :debug="app.debug"
          :deleting-key="deletingId"
          @delete="deleteRow"
        />
        <v-pagination v-if="totalCount > PAGE_SIZE" :page="currentPage" :go="gotoPage" :total="totalCount" :limit="PAGE_SIZE" :page-size="PAGE_SIZE" />
        <div v-if="rows.length === 0" class="state-wrap">
          <i-lucide:database class="state-icon" />
          <span class="state-text">{{ $t('no_data') }}</span>
        </div>
      </template>
      <div v-else class="state-wrap">
        <i-lucide:database class="state-icon" />
        <span class="state-text">{{ $t('no_data') }}</span>
      </div>
    </div>
  </template>
  <div v-else class="state-wrap">
    <i-lucide:database class="state-icon" />
    <span class="state-text">{{ $t('no_data') }}</span>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onActivated, onDeactivated } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { initQuery, initLazyQuery, dbTablesGQL, dbTableRowCountGQL, dbTableRowsGQL, dbTableInfoGQL, dbTableColumnsGQL, dbPathGQL } from '@/lib/api/query'
import { initMutation, deleteDbTableRowsGQL } from '@/lib/api/mutation'
import DevDataTable from './DevDataTable.vue'

const PAGE_SIZE = 50

const isActive = ref(false)
onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })

const { app } = storeToRefs(useTempStore())

const tables = ref<string[]>([])
const activeTable = ref('')
const rows = ref<Record<string, string | null>[]>([])
const totalCount = ref(0)
const offset = ref(0)
const deletingId = ref('')
const idKey = ref('id')
const infoColumns = ref<string[]>([])
const dbPath = ref('')
const pathOpen = ref(false)

initQuery({
  handle(data: { dbPath: string }, error: string) {
    if (!error && data?.dbPath) dbPath.value = data.dbPath
  },
  document: dbPathGQL,
})

const columns = computed(() =>
  infoColumns.value.length > 0
    ? infoColumns.value
    : rows.value.length > 0
      ? Object.keys(rows.value[0])
      : [],
)
const currentPage = computed(() => Math.floor(offset.value / PAGE_SIZE) + 1)

const { loading: tablesLoading, refetch: refetchTables } = initQuery({
  handle(data: { dbTables: string[] }, error: string) {
    if (!error) {
      tables.value = data?.dbTables ?? []
      if (tables.value.length > 0 && !activeTable.value) {
        activeTable.value = tables.value[0]
      }
    }
  },
  document: dbTablesGQL,
})

const { fetch: fetchCount } = initLazyQuery({
  handle(data: { dbTableRowCount: number }, error: string) {
    if (!error) totalCount.value = data?.dbTableRowCount ?? 0
  },
  document: dbTableRowCountGQL,
})

const { loading: rowsLoading, fetch: fetchRows } = initLazyQuery({
  handle(data: { dbTableRows: string[] }, error: string) {
    if (!error) {
      rows.value = (data?.dbTableRows ?? []).map((s) => JSON.parse(s) as Record<string, string | null>)
    }
  },
  document: dbTableRowsGQL,
})

const { fetch: fetchTableInfo } = initLazyQuery({
  handle(data: { dbTableInfo: { idKey: string } | null }, error: string) {
    if (!error && data?.dbTableInfo?.idKey) {
      idKey.value = data.dbTableInfo.idKey
    }
  },
  document: dbTableInfoGQL,
})

const { loading: columnsLoading, fetch: fetchColumns } = initLazyQuery({
  handle(data: { dbTableColumns: string[] }, error: string) {
    if (!error) infoColumns.value = data?.dbTableColumns ?? []
  },
  document: dbTableColumnsGQL,
})

function loadTable(table: string) {
  if (!table) return
  fetchTableInfo({ table })
  fetchCount({ table })
  fetchColumns({ table })
  fetchRows({ table, offset: offset.value, limit: PAGE_SIZE })
}

async function refreshData() {
  await refetchTables()
  if (activeTable.value) loadTable(activeTable.value)
}

function selectTable(table: string) {
  if (activeTable.value === table) return
  activeTable.value = table
  offset.value = 0
  rows.value = []
  infoColumns.value = []
}

watch(activeTable, (t) => {
  if (t) loadTable(t)
})

function gotoPage(page: number) {
  offset.value = (page - 1) * PAGE_SIZE
  loadTable(activeTable.value)
}

const { mutate: deleteMutate } = initMutation({ document: deleteDbTableRowsGQL })

async function deleteRow(id: string) {
  if (!id) return
  deletingId.value = id
  await deleteMutate({ table: activeTable.value, ids: [id] })
  deletingId.value = ''
  loadTable(activeTable.value)
}
</script>

<style lang="scss" scoped>
.tabs-wrap {
  padding: 8px 16px 0;
  overflow-x: auto;
}

.scroll-content {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
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
