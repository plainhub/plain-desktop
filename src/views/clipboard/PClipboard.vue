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
      <template v-if="clipboardSync">
        <div class="section-title">{{ $t('send_to_phone_clipboard') }}</div>
        <div class="clip-composer">
          <v-text-field
            v-model="clipText"
            :label="$t('clipboard_text')"
            class="composer-input"
            :error="clipTextError"
            :error-text="$t('valid.required')"
            @keyup.enter="sendToPhone"
          >
            <template #trailing-icon>
              <v-icon-button v-tooltip="$t('paste')" @click.prevent="pasteClipboardText">
                <i-material-symbols:content-paste-rounded />
              </v-icon-button>
            </template>
          </v-text-field>
          <v-filled-button class="send-btn" :loading="setClipLoading" @click.prevent="sendToPhone">
            {{ $t('send') }}
          </v-filled-button>
        </div>
      </template>
      <section v-if="items.length" class="clip-card">
        <clipboard-item
          v-for="item in items"
          :key="item.id"
          :item="item"
          @delete="deleteItem"
        />
      </section>
      <NoDataPlaceholder
        v-if="!items.length"
        :loading="loading"
        :placeholder-key="clipboardSync ? '' : 'clipboard_sync_disabled'"
        feature="CLIPBOARD"
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

const { items, total, page, limit, loading, clipboardSync, load, open, gotoPage, onChangePageSize, deleteItem, clipText, clipTextError, setClipLoading, pasteClipboardText, sendToPhone } = useClipboardData()

watch(() => store.quick === 'clipboard', (visible) => {
  if (visible) open()
})

watch(clipboardSync, (enabled) => {
  if (enabled && store.quick === 'clipboard') load()
})
</script>

<style lang="scss" scoped>
.clip-composer {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  margin: 4px 16px 8px;

  .composer-input {
    flex: 1;
    min-width: 0;
  }

  .send-btn {
    margin-top: 8px;
    min-width: 80px;
  }
}

.clip-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 16px;
}
</style>
