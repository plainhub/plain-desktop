<template>
  <v-dropdown v-if="devices.length > 1" v-model="open" class="leading" strategy="below" @mousedown.stop>
    <template #trigger>
      <v-text-button
        :aria-label="`${$t('devices')}: ${label}`"
        :aria-expanded="open"
      >
        <span>{{ label }}</span>
        <i-lucide:chevron-down />
      </v-text-button>
    </template>
    <div class="dropdown-item" @click="search.selectAll(!allSelected)">
      <v-checkbox
        :label="$t('global_search_all_devices')" :checked="allSelected" :indeterminate="!allSelected && targets.length > 0"
        @click.stop @update:model-value="search.selectAll" />
      <span>{{ $t('global_search_all_devices') }}</span>
    </div>
    <div v-for="device in devices" :key="device.id" class="dropdown-item" @click="search.select(device.id, !selectedIds.has(device.id))">
      <v-checkbox
        :label="device.name" :checked="selectedIds.has(device.id)"
        @click.stop @update:model-value="search.select(device.id, $event)" />
      <span>{{ device.name }}</span>
    </div>
  </v-dropdown>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { GlobalSearch } from '@/hooks/global-search'

const { search } = defineProps<{ search: GlobalSearch }>()
const { t } = useI18n()
const { devices, targets } = search
const open = ref(false)
const selectedIds = computed(() => new Set(targets.value.map(({ id }) => id)))
const allSelected = computed(() => targets.value.length === devices.value.length)
const label = computed(() => {
  if (allSelected.value) return t('global_search_all_devices')
  if (targets.value.length === 1) return targets.value[0].name
  return t('global_search_devices', { count: targets.value.length })
})
</script>
