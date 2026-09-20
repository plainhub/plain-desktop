<template>
  <article class="item notification-item">
    <v-dropdown v-model="iconMenuOpen">
      <template #trigger>
        <img class="app-ico" width="40" height="40" :src="item.icon" alt="">
      </template>
      <pre class="view-raw">{{ item }}</pre>
    </v-dropdown>
    <div class="ntf-main">
      <div class="row1">
        <span class="name">{{ item.appName }}</span>
        <time v-tooltip="formatDateTimeFull(item.postedAt)" class="nowrap">{{ formatTimeAgo(item.postedAt) }}</time>
        <button v-if="deletable" v-tooltip="$t('delete')" class="btn-icon del" @click.stop="$emit('delete')">
          <i-material-symbols:close-rounded />
        </button>
      </div>
      <div class="ntf-title">{{ item.title }}</div>
      <div class="ntf-body">{{ item.body }}</div>
      <div v-if="item.replyActions && item.replyActions.length && !replying" class="reply-actions">
        <v-outlined-button
          v-for="(label, idx) in item.replyActions"
          :key="idx"
          @click.stop="$emit('reply', idx)"
        >
          {{ label }}
        </v-outlined-button>
      </div>
      <div v-if="replying" class="reply-box">
        <EmojiTextField v-model="replyText" type="textarea" :rows="2" :placeholder="$t('type_a_reply')" />
        <div class="reply-box-actions">
          <v-outlined-button @click.stop="$emit('cancel-reply')">{{ $t('cancel') }}</v-outlined-button>
          <v-filled-button
            :loading="sending"
            :disabled="!replyText.trim()"
            @click.stop="$emit('send', replyText.trim())"
          >
            {{ $t('send') }}
          </v-filled-button>
        </div>
      </div>
    </div>
  </article>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { formatDateTimeFull, formatTimeAgo } from '@/lib/format'
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

const replyText = ref('')
const iconMenuOpen = ref(false)

watch(
  () => props.replying,
  (open) => {
    if (open) replyText.value = ''
  },
)
</script>

<style lang="scss" scoped>
.item.notification-item {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px;
  border-radius: var(--pl-shape-m);
  background: var(--md-sys-color-surface-container-low);
  word-break: break-all;

  &:hover {
    background: var(--md-sys-color-surface-container-high);
  }

  .app-ico {
    width: 40px;
    height: 40px;
    border-radius: var(--pl-shape-s);
    object-fit: cover;
    flex-shrink: 0;
  }

  .ntf-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .row1 {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .name {
    flex: 0 1 auto;
    min-width: 0;
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--md-sys-color-on-surface);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  time {
    font-size: 0.75rem;
    color: var(--md-sys-color-on-surface-variant);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .del {
    margin-left: auto;
    width: 28px;
    height: 28px;
    flex: 0 0 28px;
    opacity: 0;
    pointer-events: none;

    svg {
      width: 16px;
      height: 16px;
    }
  }

  &:hover .del {
    opacity: 1;
    pointer-events: auto;
  }

  .ntf-title {
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--md-sys-color-on-surface);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ntf-body {
    font-size: 0.8rem;
    color: var(--md-sys-color-on-surface-variant);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .reply-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
  }

  .reply-box {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .reply-box-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  @media (hover: none) {
    .del {
      opacity: 1;
      pointer-events: auto;
    }
  }
}
</style>
