<template>
  <article class="item clipboard-item">
    <div class="clip-main">
      <div class="row1">
        <span v-if="hasName" class="name">{{ item.label || item.source }}</span>
        <time v-tooltip="formatDateTimeFull(item.createdAt)" class="nowrap" :class="{ 'time-solo': !hasName }">{{ formatTimeAgo(createdAt) }}</time>
        <v-copy-button v-tooltip="$t('copy')" class="btn-icon copy" :text="item.text">
          <template #icon>
            <i-material-symbols:content-copy-outline-rounded />
          </template>
        </v-copy-button>
        <button v-tooltip="$t('delete')" class="btn-icon del" @click.stop="$emit('delete', item)">
          <i-material-symbols:close-rounded />
        </button>
      </div>
      <div class="clip-text" :class="{ sensitive: item.sensitive, collapsed: isLong && !expanded }">{{ item.text }}</div>
      <a v-if="isLong" href="#" class="show-more" @click.prevent="expanded = !expanded">
        {{ $t(expanded ? 'show_less' : 'show_more') }}
      </a>
    </div>
  </article>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { formatDateTimeFull, formatTimeAgo } from '@/lib/format'
import type { IClipboardItem } from '@/lib/interfaces'

const props = defineProps<{ item: IClipboardItem }>()

defineEmits<{
  delete: [item: IClipboardItem]
}>()

const LONG_TEXT_CHARS = 250

const expanded = ref(false)

const hasName = computed(() => !!(props.item.label || props.item.source))
const isLong = computed(() => (props.item.text?.length ?? 0) > LONG_TEXT_CHARS)

const createdAt = computed(() => {
  const v = props.item.createdAt
  return /^\d+$/.test(v ?? '') ? new Date(Number(v)).toISOString() : v
})
</script>

<style lang="scss" scoped>
.item.clipboard-item {
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

  .clip-main {
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

  .time-solo {
    margin-right: auto;
  }

  .copy,
  .del {
    width: 28px;
    height: 28px;
    flex: 0 0 28px;
    opacity: 0;
    pointer-events: none;
  }

  .copy {
    font-size: 16px;
    transition: color 0.15s ease;

    &.copied {
      opacity: 1;
      pointer-events: auto;
    }
  }

  .del svg {
    width: 16px;
    height: 16px;
  }

  &:hover .copy,
  &:hover .del {
    opacity: 1;
    pointer-events: auto;
  }

  .clip-text {
    font-size: 0.8rem;
    color: var(--md-sys-color-on-surface-variant);
    white-space: pre-wrap;

    &.collapsed {
      display: -webkit-box;
      -webkit-line-clamp: 3;
      -webkit-box-orient: vertical;
      overflow: hidden;
    }

    &.sensitive {
      -webkit-line-clamp: 1;
      filter: blur(4px);
    }
  }

  .show-more {
    align-self: flex-start;
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--md-sys-color-primary);
    text-decoration: none;

    &:hover {
      text-decoration: underline;
    }
  }

  @media (hover: none) {
    .copy,
    .del {
      opacity: 1;
      pointer-events: auto;
    }
  }
}
</style>
