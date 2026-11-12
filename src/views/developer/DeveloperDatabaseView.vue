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
  <div class="database-layout">
    <DeveloperDatabaseSidebar
      :tables="tables"
      :active-table="activeTable"
      :loading="tablesLoading"
      @select="selectTable"
    />
    <section class="database-content">
      <div v-if="tablesLoading && tables.length === 0" class="state-wrap">
        <v-circular-progress indeterminate />
      </div>
      <template v-else-if="tables.length > 0">
        <div class="scroll-content">
          <div v-if="rowsLoading && columns.length === 0" class="state-wrap">
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
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onActivated, onDeactivated } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { initQuery, initLazyQuery, dbTablesGQL, dbTableRowCountGQL, dbTableRowsGQL, dbPathGQL } from '@/lib/api/query'
import type { GqlResultType } from '@/lib/api/graphql/operations'
import { initMutation, deleteDbTableRowsGQL } from '@/lib/api/mutation'
import DevDataTable from './DevDataTable.vue'
import DeveloperDatabaseSidebar from './DeveloperDatabaseSidebar.vue'

const PAGE_SIZE = 50

const isActive = ref(false)
onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })

const { app } = storeToRefs(useTempStore())

const tables = ref<GqlResultType<'dbTablesGQL'>['dbTables']>([])
const activeTable = ref('')
const rows = ref<Record<string, string | null>[]>([])
const totalCount = ref(0)
const offset = ref(0)
const deletingId = ref('')
const selectedTable = computed(() => tables.value.find((table) => table.name === activeTable.value))
const idKey = computed(() => selectedTable.value?.columns.find((column) => column.primaryKey)?.name ?? 'id')
const infoColumns = computed(() => selectedTable.value?.columns.map((column) => column.name) ?? [])
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
  handle(data: GqlResultType<'dbTablesGQL'>, error: string) {
    if (!error) {
      tables.value = data?.dbTables ?? []
      if (!tables.value.some((table) => table.name === activeTable.value)) {
        selectTable(tables.value[0]?.name ?? '')
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

function loadTable(table: string) {
  if (!table) return
  fetchCount({ table }, { latest: true })
  fetchRows({ table, offset: offset.value, limit: PAGE_SIZE }, { latest: true })
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
  totalCount.value = 0
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
.database-layout {
  display: flex;
  flex: 1;
  min-height: 0;
}

.database-content {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
  min-height: 0;
}

.scroll-content {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.state-wrap {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
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
