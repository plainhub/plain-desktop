<template>
  <left-sidebar>
    <template #body>
      <ul class="nav">
        <SidebarListItem
          :title="$t('all')"
          :active="!selectedTagId && !selectedBucketId && !trash && !selectedExt"
          @click="viewAll"
        >
          <template #start>
            <i-lucide:layout-grid />
          </template>
          <template v-if="total >= 0" #end>
            <span class="count">{{ total.toLocaleString() }}</span>
          </template>
        </SidebarListItem>
        <SidebarListItem
          v-if="hasMediaTrash(app)"
          :title="$t('trash')"
          :active="trash"
          @click="viewTrash"
        >
          <template #start>
            <i-lucide:trash />
          </template>
          <template #end>
            <v-icon-button v-tooltip="$t('trash_tips')" class="btn-help sm">
              <i-material-symbols:help-outline-rounded />
            </v-icon-button>
            <span v-if="totalTrash >= 0" class="count">{{ totalTrash.toLocaleString() }}</span>
          </template>
        </SidebarListItem>
      </ul>
      <ext-filter v-if="props.type === DataType.DOC" :ext-groups="extGroups" :selected="selectedExt" :view-by-ext="viewByExt" />
      <bucket-filter :type="props.type" :selected="selectedBucketId" />
      <tag-filter :type="props.type" :selected="selectedTagId" />
    </template>
  </left-sidebar>
</template>

<script setup lang="ts">
import type { PropType } from 'vue'
import { DataType } from '@/lib/data'
import { hasMediaTrash } from '@/lib/feature'
import { useMediaSidebar } from '@/hooks/media-sidebar'

const props = defineProps({
  type: { type: String as PropType<DataType>, required: true },
  gql: { type: String, required: true },
})

const { app, total, totalTrash, trash, selectedTagId, selectedBucketId, selectedExt, extGroups, viewAll, viewTrash, viewByExt } = useMediaSidebar(props.type as DataType, props.gql)
</script>
