<template>
  <div class="quick-content-main">
    <div class="top-app-bar">
      <button v-tooltip="$t('close')" class="btn-icon" @click.prevent="store.quick = ''">
        <i-material-symbols:arrow-back-rounded />
      </button>
      <div class="title">
        {{ $t('header_actions.clipboard') }}
        <span v-if="total" class="count-pill">{{ total.toLocaleString() }}</span>
      </div>
      <div class="actions"></div>
    </div>

    <div class="quick-content-body">
      <section v-if="items.length" class="clip-card">
        <clipboard-item
          v-for="item in items"
          :key="item.id"
          :item="item"
          @delete="deleteItem"
        />
      </section>
      <NoDataPlaceholder
        v-else
        :loading="loading"
        :placeholder-key="clipboardSync ? '' : 'clipboard_sync_disabled'"
        feature="CLIPBOARD_SYNC"
      />
      <v-pagination
        v-if="total > limit"
        :page="page"
        :go="gotoPage"
        :total="total"
        :limit="limit"
        :page-size="limit"
        :on-change-page-size="onChangePageSize"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { watch } from 'vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import ClipboardItem from '@/components/ClipboardItem.vue'
import { useMainStore } from '@/stores/main'
import { useClipboardData } from './clipboard'

const store = useMainStore()

const { items, total, page, limit, loading, clipboardSync, load, open, gotoPage, onChangePageSize, deleteItem } = useClipboardData()

watch(() => store.quick === 'clipboard', (visible) => {
  if (visible) open()
})

watch(clipboardSync, (enabled) => {
  if (enabled && store.quick === 'clipboard') load()
})
</script>

<style lang="scss" scoped>
.clip-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 16px;
}
</style>
