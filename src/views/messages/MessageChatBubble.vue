<!-- eslint-disable vue/no-v-html -->
<template>
  <div
    class="chat-bubble-row"
    :class="{ sent: isSent, received: !isSent }"
  >
    <div class="chat-bubble">
      <div v-if="item.body" v-html="addLinksToURLs(item.body)"></div>
      <div v-if="item.attachments?.length" class="chat-attachments">
        <template v-for="(att, idx) in item.attachments" :key="`${item.id}-att-${idx}`">
          <img v-if="att.contentType.startsWith('image/')" class="chat-attachment-image" :src="resolveUrl(att.path)" :alt="att.name || 'mms-image'" loading="lazy" decoding="async" />
          <video v-else-if="att.contentType.startsWith('video/')" class="chat-attachment-video" :src="resolveUrl(att.path)" controls preload="metadata" />
          <audio v-else-if="att.contentType.startsWith('audio/')" class="chat-attachment-audio" :src="resolveUrl(att.path)" controls preload="metadata" />
          <a v-else class="chat-attachment-link" :href="resolveUrl(att.path)" target="_blank" rel="noopener noreferrer">
            {{ att.name || att.contentType || att.path }}
          </a>
        </template>
      </div>
    </div>
    <div v-if="item.tags?.length" class="chat-bubble-tags">
      <span v-for="tag in item.tags" :key="tag.id" class="chat-tag-chip">{{ tag.name }}</span>
    </div>
    <div class="chat-bubble-meta">
      <span v-tooltip="formatDateTime(item.sentAt)" class="chat-time">{{ formatTime(item.sentAt) }}</span>
      <template v-if="!isDraftOrPending">
        <TagRelationsDropdown :type="type" :tags="tags" :item="{ key: item.id }" :selected="item.tags ?? []" />
        <v-icon-button v-tooltip="$t('trash_message')" class="sm" @click.stop="emit('trash', item)">
          <i-material-symbols:delete-outline-rounded />
        </v-icon-button>
      </template>
    </div>
    <span v-if="isDraftOrPending" class="chat-pending-status" :class="{ failed: pendingFailed }">
      <i-material-symbols:error-outline-rounded v-if="pendingFailed" class="pending-error-icon" />
      {{ $t(pendingStatusKey) }}
    </span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ISms, ITag } from '@/lib/interfaces'
import { formatDateTime, formatTime } from '@/lib/format'
import { addLinksToURLs } from '@/lib/strutil'
import { getFileUrlByPath } from '@/lib/api/file'
import { getPendingSmsSendState } from '@/lib/sms-state-sync'

const props = defineProps<{
  item: ISms
  tags: ITag[]
  type: string
  urlTokenKey: Uint8Array | null
}>()

const emit = defineEmits<{
  trash: [item: ISms]
}>()

const isSent = computed(() => props.item.type === 'SENT' || props.item.type === 'OUTBOX')
const sendState = computed(() => getPendingSmsSendState(props.item))
const isDraftOrPending = computed(() => sendState.value !== undefined || props.item.type === 'DRAFT')
const pendingFailed = computed(() => {
  if (sendState.value !== 'sending' || !props.item.isMms) return false
  return Date.now() - new Date(props.item.sentAt).getTime() > 5 * 60 * 1000
})
const pendingStatusKey = computed(() => {
  if (pendingFailed.value) return 'mms_canceled'
  return sendState.value ?? 'message_type.DRAFT'
})

function resolveUrl(path: string): string {
  if (path.startsWith('blob:') || path.startsWith('data:')) return path
  return getFileUrlByPath(props.urlTokenKey, path)
}
</script>

<style scoped lang="scss">
.chat-bubble-row {
  display: flex;
  flex-direction: column;
  max-width: 75%;

  .chat-bubble {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }

  &.sent {
    align-self: flex-end;
    align-items: flex-end;

    .chat-bubble {
      border-radius: 16px 16px 4px 16px;
    }
  }

  &.received {
    align-self: flex-start;
    align-items: flex-start;

    .chat-bubble {
      border-radius: 16px 16px 16px 4px;
    }
  }
}

.chat-bubble {
  padding: 10px 14px;
  font-size: 0.9375rem;
  line-height: 1.5;
  word-break: break-word;
  white-space: pre-wrap;
}

.chat-attachments {
  margin-top: 8px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.chat-attachment-image,
.chat-attachment-video {
  max-width: min(360px, 100%);
  border-radius: 12px;
  display: block;
}

.chat-attachment-audio {
  width: min(360px, 100%);
}

.chat-attachment-link {
  display: inline-flex;
  color: inherit;
  text-decoration: underline;
  text-underline-offset: 2px;
}

.chat-bubble-meta {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-top: 4px;
}

.chat-time {
  font-size: 0.6875rem;
  color: var(--md-sys-color-on-surface-variant);
  margin: 0;
  padding-inline: 4px;
  white-space: nowrap;
}

.chat-pending-status {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 0.6875rem;
  color: var(--md-sys-color-on-surface-variant);
  padding-inline: 4px;
  margin-top: 2px;

  &.failed {
    color: var(--md-sys-color-error);
  }

  .pending-error-icon {
    font-size: 14px;
  }
}

.chat-bubble-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 4px;
  padding-inline: 4px;
}

.chat-tag-chip {
  font-size: 0.6875rem;
  color: var(--md-sys-color-on-secondary-container);
  background: var(--md-sys-color-secondary-container);
  padding: 2px 8px;
  border-radius: 10px;
  white-space: nowrap;
}
</style>
