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
        <div class="clip-composer">
          <v-text-field
            v-model="clipText"
            :placeholder="$t('send_to_phone_clipboard')"
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
      <clipboard-history v-if="items.length" :items="items" @delete="deleteItem" />
      <NoDataPlaceholder
        v-if="!items.length"
        :loading="loading"
        :access="clipboardSync ? ['CLIPBOARD'] : []"
        :placeholder-key="clipboardSync ? listError : 'clipboard_disabled_on_device'"
        :show-settings-link="clipboardSync ? undefined : true"
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
import ClipboardHistory from './ClipboardHistory.vue'
import { useMainStore } from '@/stores/main'
import { useClipboardData } from './clipboard'

const store = useMainStore()

const { items, listError, total, page, limit, loading, clipboardSync, load, open, gotoPage, onChangePageSize, deleteItem, clipText, clipTextError, setClipLoading, pasteClipboardText, sendToPhone } = useClipboardData()

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
  margin: 16px 16px 8px;

  .composer-input {
    flex: 1;
    min-width: 0;
  }

  .send-btn {
    flex: none;
    margin-top: 8px;
    min-width: 80px;
    padding-inline: 24px;
    white-space: nowrap;

    :deep(.v-circular-progress) {
      position: absolute;
      inset-inline-start: 4px;
      --size: 16px;
    }
  }
}

</style>
