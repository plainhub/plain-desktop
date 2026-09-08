<template>
  <div class="item notification-item">
    <div class="title">
      <v-dropdown v-model="iconMenuOpen">
        <template #trigger>
          <img width="20" height="20" :src="item.icon" />
        </template>
        <pre class="view-raw">{{ item }}</pre>
      </v-dropdown>
      <span class="name">{{ item.appName }}</span>
      <time v-tooltip="formatDateTimeFull(item.time)" class="time nowrap">{{ formatDateTime(item.time) }}</time>
    </div>
    <div class="subtitle">{{ item.title }}</div>
    <div class="body">{{ item.body }}</div>
    <div v-if="item.replyActions && item.replyActions.length && !replying" class="reply-actions">
      <v-outlined-button
        v-for="(label, idx) in item.replyActions"
        :key="idx"
        class="btn-sm"
        @click.stop="$emit('reply', idx)"
      >
        {{ label }}
      </v-outlined-button>
    </div>
    <div v-if="replying" class="reply-box">
      <EmojiTextField v-model="replyText" type="textarea" :rows="2" :placeholder="$t('type_a_reply')" />
      <div class="reply-box-actions">
        <v-outlined-button class="btn-sm" @click.stop="$emit('cancel-reply')">{{ $t('cancel') }}</v-outlined-button>
        <v-filled-button
          class="btn-sm"
          :loading="sending"
          :disabled="!replyText.trim()"
          @click.stop="$emit('send', replyText.trim())"
        >
          {{ $t('send') }}
        </v-filled-button>
      </div>
    </div>
    <button v-if="deletable" v-tooltip="$t('delete')" class="btn-icon icon" @click.stop="$emit('delete')">
      <i-material-symbols:close-rounded />
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { formatDateTime, formatDateTimeFull } from '@/lib/format'
import type { INotification } from '@/lib/interfaces'

const props = withDefaults(
  defineProps<{
    item: INotification
    replying: boolean
    sending: boolean
    /** Offline sources cannot action deletes — consumers hide the button. */
    deletable?: boolean
  }>(),
  { deletable: true },
)

defineEmits<{
  reply: [actionIndex: number]
  'cancel-reply': []
  send: [text: string]
  delete: []
}>()

const iconMenuOpen = ref(false)
const replyText = ref('')

watch(
  () => props.replying,
  (open) => {
    if (open) replyText.value = ''
  },
)
</script>

<style lang="scss" scoped>
/* Extends the global `.list-items .item` grid (3 areas) with reply rows;
   the doubled class keeps this above the global specificity. */
.item.notification-item {
  grid-template-areas:
    'title icon'
    'subtitle icon'
    'body body'
    'reply-actions reply-actions'
    'reply-box reply-box';

  &:first-child {
    margin-block-start: 8px;
  }

  &:last-child {
    margin-block-end: 8px;
  }

  .title img {
    margin-inline-end: 8px;
  }

  .time {
    color: var(--md-sys-color-secondary);
    font-size: 0.75rem;
    margin-inline-start: 8px;
    word-break: keep-all;
    white-space: nowrap;
  }

  .name {
    word-break: keep-all;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .reply-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
  }

  .reply-box {
    grid-area: reply-box;
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .reply-box-actions {
    grid-area: reply-actions;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
}
</style>
