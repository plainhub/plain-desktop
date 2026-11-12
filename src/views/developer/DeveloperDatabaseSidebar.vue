<template>
  <Sidebar2 class="database-sidebar" detail>
    <div class="tables-heading">{{ $t('developer.database') }} ({{ tables.length }})</div>
    <div class="tables-list scroller">
      <v-circular-progress v-if="loading && tables.length === 0" indeterminate />
      <div v-for="table in tables" :key="table.name" class="table-item selectable-card" :class="{ selected: activeTable === table.name }">
        <button class="table-name" :aria-pressed="activeTable === table.name" @click="$emit('select', table.name)">{{ table.name }}</button>
        <v-dropdown :model-value="helpTable === table.name" @update:model-value="(open: boolean) => helpTable = open ? table.name : ''">
          <template #trigger>
            <v-icon-button :aria-label="`${table.name}: CREATE TABLE`" v-tooltip="'CREATE TABLE'">
              <i-material-symbols:help-outline-rounded />
            </v-icon-button>
          </template>
          <pre class="view-raw">{{ table.createSql }}</pre>
        </v-dropdown>
      </div>
      <NoDataPlaceholder v-if="!loading && tables.length === 0" />
    </div>
  </Sidebar2>
</template>

<script setup lang="ts">
import { onDeactivated, ref } from 'vue'
import type { GqlResultType } from '@/lib/api/graphql/operations'

defineProps<{ tables: GqlResultType<'dbTablesGQL'>['dbTables']; activeTable: string; loading: boolean }>()
defineEmits<{ select: [table: string] }>()

const helpTable = ref('')
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
