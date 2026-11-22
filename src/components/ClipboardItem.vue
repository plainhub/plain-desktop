<template>
  <article class="item clipboard-item">
    <div class="clip-main">
      <div class="row1">
        <v-dropdown v-model="rawOpen" class="clip-time" borderless>
          <template #trigger>
            <button
              v-tooltip="formatDateTimeFull(createdAt)"
              type="button"
              class="clip-time-button"
              aria-haspopup="dialog"
              :aria-expanded="rawOpen"
              @keydown.esc.stop="rawOpen = false"
            >
              <time :datetime="createdAt">{{ formatTimeAgo(createdAt) }}</time>
            </button>
          </template>
          <pre class="view-raw" role="dialog" :aria-label="$t('session_management.view_raw')" tabindex="0" @keydown.esc.stop="rawOpen = false">{{ item }}</pre>
        </v-dropdown>
        <v-copy-button v-tooltip="$t('copy')" class="btn-icon copy" :text="item.text">
          <template #icon>
            <i-material-symbols:content-copy-outline-rounded />
          </template>
        </v-copy-button>
        <button v-tooltip="$t('delete')" type="button" class="btn-icon del" :aria-label="$t('delete')" @click.stop="$emit('delete', item)">
          <i-material-symbols:delete-outline-rounded />
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
import { computed, onDeactivated, ref } from 'vue'
import VDropdown from '@/components/base/VDropdown.vue'
import { formatDateTimeFull, formatTimeAgo } from '@/lib/format'
import type { IClipboardItem } from '@/lib/interfaces'

const props = defineProps<{ item: IClipboardItem }>()

defineEmits<{
  delete: [item: IClipboardItem]
}>()

const LONG_TEXT_CHARS = 250

const expanded = ref(false)

const rawOpen = ref(false)
onDeactivated(() => { rawOpen.value = false })
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

  .clip-time {
    margin-inline-end: auto;
  }

  .clip-time-button {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    font-size: 0.75rem;
    color: var(--md-sys-color-on-surface-variant);
    white-space: nowrap;
  }

  .copy,
  .del {
    width: 28px;
    height: 28px;
    flex: 0 0 28px;
    opacity: 0;
    pointer-events: none;
  }

  &:is(:hover, :focus-within) :is(.copy, .del) {
    opacity: 1;
    pointer-events: auto;
  }

  :is(.copy, .del):focus-visible {
    outline: 2px solid var(--md-sys-color-primary);
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
