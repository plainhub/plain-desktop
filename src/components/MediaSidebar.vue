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
          v-if="hasMediaTrash(app, props.type)"
          class="media-trash-row"
          :title="$t('trash')"
          :active="trash"
          @click="viewTrash"
        >
          <template #start>
            <i-lucide:trash />
          </template>
          <template v-if="totalTrash >= 0" #end>
            <span v-if="isLocalMode()" class="count">{{ totalTrash.toLocaleString() }}</span>
            <button
              v-else
              v-tooltip="$t('trash_tips')"
              type="button"
              class="count btn-help trash-count"
              :aria-label="`${$t('trash')}: ${totalTrash.toLocaleString()}. ${$t('trash_tips')}`"
            >
              <span>{{ totalTrash.toLocaleString() }}</span>
              <i-material-symbols:help-outline-rounded class="trash-count-icon" aria-hidden="true" />
            </button>
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
import type { MediaDataType } from '@/lib/media/library-updates'
import { isLocalMode } from '@/lib/device/local-mode'
import { hasMediaTrash } from '@/lib/feature'
import { useMediaSidebar } from '@/hooks/media-sidebar'

const props = defineProps({
  type: { type: String as PropType<Extract<DataType, MediaDataType>>, required: true },
  gql: { type: String, required: true },
})

const { app, total, totalTrash, trash, selectedTagId, selectedBucketId, selectedExt, extGroups, viewAll, viewTrash, viewByExt } = useMediaSidebar(props.type, props.gql)
</script>

<style lang="scss" scoped>
.trash-count {
  display: inline-grid;
  place-items: center;
  min-block-size: 32px;
  padding: 0;
  border: 0;
  background: none;
  color: inherit;
  font: inherit;
  cursor: inherit;

  > * {
    grid-area: 1 / 1;
  }

  .trash-count-icon {
    width: 20px;
    height: 20px;
    visibility: hidden;
  }
}

@media (hover: hover) {
  .media-trash-row:hover .trash-count {
    > span { visibility: hidden; }
    .trash-count-icon { visibility: visible; }
  }
}
</style>
