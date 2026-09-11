<template>
  <div class="quick-content-main">
    <div class="top-app-bar">
      <button v-tooltip="$t('close')" class="btn-icon" @click="store.quick = ''">
        <i-material-symbols:arrow-back-rounded />
      </button>
      <div class="title">{{ $t('header_actions.uploads') }}</div>
    </div>

    <div class="quick-content-body">
      <div class="filter-bar">
        <div class="button-group">  
          <button v-for="type in types" :key="type" :class="{ 'selected': filterType === type }" @click="chooseFilterType(type)">
            {{ getLabel(type) }}
          </button>
        </div>
      </div>
      <VirtualList ref="listItemsRef" class="list-items" :data-key="'id'" :data-sources="visibleTasks" :estimate-size="80">
        <template #item="{ item }">
          <UploadBatchTaskItem :key="item.id" :batch-id="item.batchId" />
        </template>
      </VirtualList>

      <div v-if="!visibleTasks.length" class="no-data">
        <div class="empty-content">
          <div class="empty-text">{{ $t('no_task') }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import VirtualList from '@/components/virtualscroll'
import UploadBatchTaskItem from '@/components/UploadBatchTaskItem.vue'
import { useUploadList } from './upload-list'

const { store, filterType, types, listItemsRef, visibleTasks, chooseFilterType, getLabel } = useUploadList()
</script>

<style scoped lang="scss">
.quick-content-body {
  display: flex;
  flex-direction: column;
}

.filter-bar {
  padding: 8px 16px;

  .button-group {
    width: 100%;
  }
}

.list-items {
  padding-block: 8px;
  overflow-y: auto;
  overflow-x: hidden;
  flex: 1;
  min-height: 0;
}

.empty-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  color: var(--md-sys-color-on-surface-variant);
}

.empty-text {
  font-size: 1rem;
  opacity: 0.7;
}
</style>
