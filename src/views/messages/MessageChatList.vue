<template>
  <div ref="scrollRef" class="chat-messages" @scroll="$emit('scroll')">
    <div v-if="detailLoading" class="chat-loading">
      <v-circular-progress indeterminate class="sm" />
    </div>
    <div v-if="loadingMore" class="chat-loading-more">
      <v-circular-progress indeterminate class="sm" />
    </div>
    <template v-if="!detailLoading">
      <div v-for="(item, index) in items" :key="item.id" class="chat-message-wrapper">
        <div v-if="showDateSeparator(index)" class="chat-date-separator">
          <span>{{ formatDateLabel(item.date) }}</span>
        </div>
        <MessageChatBubble :item="item" :tags="tags" :type="type" :url-token-key="urlTokenKey" @trash="(item: IMessage) => $emit('trash', item)" />
      </div>
    </template>
    <NoDataPlaceholder v-if="!detailLoading && items.length === 0" :loading="loading" :permissions="permissions" permission="READ_SMS" />
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { IMessage, ITag } from '@/lib/interfaces'
import { formatDateTime } from '@/lib/format'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import MessageChatBubble from './MessageChatBubble.vue'

const props = defineProps<{
  items: IMessage[]
  detailLoading: boolean
  loading: boolean
  loadingMore: boolean
  permissions: string[]
  urlTokenKey: Uint8Array | null
  tags: ITag[]
  type: string
}>()

defineEmits<{
  scroll: []
  trash: [item: IMessage]
}>()

const scrollRef = defineModel<HTMLElement | undefined>('scrollRef')

const { t } = useI18n()

function showDateSeparator(index: number): boolean {
  if (index === 0) return true
  const current = new Date(props.items[index].date).toDateString()
  const prev = new Date(props.items[index - 1].date).toDateString()
  return current !== prev
}

function formatDateLabel(dateStr: string): string {
  const date = new Date(dateStr)
  const today = new Date()
  const yesterday = new Date()
  yesterday.setDate(yesterday.getDate() - 1)
  if (date.toDateString() === today.toDateString()) return t('today')
  if (date.toDateString() === yesterday.toDateString()) return t('yesterday')
  return formatDateTime(dateStr, { dateStyle: 'medium' })
}
</script>

<style scoped lang="scss">
.chat-messages {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.chat-loading {
  display: flex;
  justify-content: center;
  align-items: center;
  flex: 1;
}

.chat-loading-more {
  display: flex;
  justify-content: center;
  padding: 12px 0;
}

.chat-date-separator {
  display: flex;
  justify-content: center;
  margin: 16px 0 8px;
  span {
    font-size: 0.75rem;
    color: var(--md-sys-color-on-surface-variant);
    background: var(--md-sys-color-surface-container-high);
    padding: 4px 12px;
    border-radius: 12px;
  }
}
</style>
