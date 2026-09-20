<template>
  <div class="main-list">
    <section
      v-for="(item, index) in items"
      :key="item.id"
      class="sms-item selectable-card"
      @click="openConversation(item.id)"
    >
      <div class="start">
        <span class="number"><field-id :id="index + 1" :raw="item" /></span>
      </div>
      <div class="title">{{ item.address || '-' }}</div>
      <div class="subtitle">{{ item.snippet || '-' }}</div>
      <div class="info">
        <span>{{ item.messageCount.toLocaleString() }}</span>
      </div>
      <div class="time">
        <span v-tooltip="formatDateTime(item.lastMessageAt)">{{ formatTimeAgo(item.lastMessageAt) }}</span>
      </div>
    </section>
    <template v-if="loading && items.length === 0">
      <MessageSkeletonItem v-for="i in 20" :key="i" :index="i" :is-phone="isPhone" />
    </template>
  </div>
</template>

<script setup lang="ts">
import type { ISmsConversation } from '@/lib/interfaces'
import { formatDateTime, formatTimeAgo } from '@/lib/format'

interface Props {
  items: ISmsConversation[]
  isPhone: boolean
  loading: boolean
}

defineProps<Props>()

const emit = defineEmits<{
  openConversation: [threadId: string]
}>()

function openConversation(threadId: string) {
  emit('openConversation', threadId)
}
</script>
