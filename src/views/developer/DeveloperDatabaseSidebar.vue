<template>
  <Sidebar2 class="database-sidebar" detail>
    <div class="tables-heading">{{ $t('page_title.database') }} ({{ tables.length }})</div>
    <div class="tables-list scroller">
      <v-circular-progress v-if="loading && tables.length === 0" indeterminate />
      <div v-for="table in tables" :key="table" class="table-item selectable-card" :class="{ selected: activeTable === table }">
        <button class="table-name" :aria-pressed="activeTable === table" @click="$emit('select', table)">{{ table }}</button>
        <v-dropdown :model-value="helpTable === table" @update:model-value="(open: boolean) => open ? showTableSchema(table) : helpTable = ''">
          <template #trigger>
            <v-icon-button :aria-label="`${table}: CREATE TABLE`" v-tooltip="'CREATE TABLE'">
              <i-material-symbols:help-outline-rounded />
            </v-icon-button>
          </template>
          <v-circular-progress v-if="schemaLoading" indeterminate />
          <div v-else-if="schemaError" class="view-raw" role="alert">{{ $t(schemaError) }}</div>
          <pre v-else class="view-raw">{{ createSql }}</pre>
        </v-dropdown>
      </div>
      <NoDataPlaceholder v-if="!loading && tables.length === 0" />
    </div>
  </Sidebar2>
</template>

<script setup lang="ts">
import { onDeactivated, ref } from 'vue'
import { dbTableInfoGQL, initLazyQuery } from '@/lib/api/query'
import type { GqlResultType } from '@/lib/api/graphql/operations'

defineProps<{ tables: string[]; activeTable: string; loading: boolean }>()
defineEmits<{ select: [table: string] }>()

const helpTable = ref('')
const createSql = ref('')
const schemaError = ref('')
const { loading: schemaLoading, fetch: fetchTableInfo } = initLazyQuery<GqlResultType<'dbTableInfoGQL'>>({
  document: dbTableInfoGQL,
  handle(data, error, context) {
    if (context?.variables?.table !== helpTable.value) return
    schemaError.value = error
    if (!error) createSql.value = data.dbTableInfo.createSql
  },
})

function showTableSchema(table: string) {
  helpTable.value = table
  createSql.value = ''
  schemaError.value = ''
  void fetchTableInfo({ table }, { latest: true })
}

onDeactivated(() => { helpTable.value = '' })
</script>

<style scoped lang="scss">
.database-sidebar {
  flex-shrink: 0;
  min-width: 0;
  max-width: 50%;
}

.tables-heading {
  padding: 16px;
  font-weight: 500;
}

.tables-list {
  padding: 0 16px 16px;
}

.table-item {
  display: flex;
  align-items: center;
  gap: 8px;
  border-radius: 8px;
  padding-inline-end: 4px;
  margin-block-end: 8px;
}

.table-name {
  flex: 1;
  min-width: 0;
  align-self: stretch;
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: start;
  overflow-wrap: anywhere;
  padding: 16px;
  cursor: pointer;
}
</style>
