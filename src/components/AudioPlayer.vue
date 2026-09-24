<template>
  <div class="quick-content-main">
    <div class="top-app-bar">
      <button v-tooltip="$t('close')" class="btn-icon" @click.prevent="store.quick = ''">
        <i-material-symbols:arrow-back-rounded />
      </button>
      <div class="title">{{ current?.title ?? $t('audio_player') }}</div>
    </div>
    <div class="quick-content-body">
      <audio ref="audioRef" class="audio" controls :src="src" @ended="onEnded" />
      <div v-if="audios.length" class="buttons">
        <button class="btn-icon" @click.stop="playPrev">
          <i-material-symbols:skip-previous-outline-rounded />
        </button>
        <button class="btn-icon mode" @click.stop="changeMode">
          <i-material-symbols:shuffle-outline-rounded v-if="playback?.mode === 'SHUFFLE'" />
          <i-material-symbols:repeat-rounded v-else-if="playback?.mode === 'REPEAT'" />
          <i-material-symbols:repeat-one-rounded v-else />
        </button>
        <button class="btn-icon" @click.stop="playNext">
          <i-material-symbols:skip-next-outline-rounded />
        </button>
        <v-icon-button v-tooltip="$t('clear_list')" :loading="clearLoading" @click.prevent="clearPlaylist">
          <i-material-symbols:delete-forever-outline-rounded />
        </v-icon-button>
      </div>
      <section ref="listEl" class="list-items">
          <div 
            v-for="(item, index) in playlistAudios" 
            :key="item.path"
            class="item" 
            :class="{ selected: item.path === current?.path, dragging: fromIndex === index }" 
            :style="itemStyle(index)"
            @click.stop="onItemClick(item)" 
            @mousedown="fixUserSelect"
            @pointerdown="start($event, index)"
          >
            <div class="item-number">{{ index + 1 }}</div>
            <div class="content">
              <div class="title">{{ item.title }}</div>
              <div class="subtitle">{{ item.artist }} {{ formatDurationMs(item.durationMs) }}</div>
            </div>
            <button v-tooltip="$t('remove_from_playlist')" class="btn-icon icon" @click.stop="deleteItem(item)">
              <i-material-symbols:playlist-remove class="playlist-remove-icon" />
            </button>
          </div>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { formatDurationMs, formatSeconds } from '@/lib/format'
import type { IAudioItem } from '@/lib/interfaces'
import { useMainStore } from '@/stores/main'
import { fixUserSelect } from '@/hooks/text-selection'
import { useAudioPlaylist } from '@/hooks/audio-player'
import { useDragSort } from '@/hooks/use-drag-sort'

const store = useMainStore()
const audioRef = ref<HTMLAudioElement>()

const {
  playback,
  audios,
  playlistAudios,
  current,
  src,
  clearLoading,
  onEnded,
  playPrev,
  playNext,
  changeMode,
  playItem,
  deleteItem,
  clearPlaylist,
  onReorder,
} = useAudioPlaylist(audioRef)

const listEl = ref<HTMLElement>()

const { fromIndex, suppressClick, itemStyle, start } = useDragSort({
  containerRef: listEl,
  itemSelector: '.item',
  onEnd(from, to) {
    const items = [...playlistAudios.value]
    const [moved] = items.splice(from, 1)
    items.splice(to, 0, moved)
    playlistAudios.value = items
    onReorder()
  },
})

function onItemClick(item: IAudioItem) {
  if (suppressClick.value) return
  playItem(item)
}
</script>

<style lang="scss" scoped>
.list-items .item {
  cursor: pointer;
  display: grid;
  grid-template-areas: 'number content icon';
  grid-template-columns: auto 1fr auto;
  gap: 12px;
  align-items: center;
  padding: 8px 16px;
  transition: transform 0.2s ease;
  will-change: transform;
  user-select: none;
}

.list-items .item.dragging {
  opacity: 0.85;
  background: var(--md-sys-color-surface-container-high);
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}

.list-items .item .item-number {
  grid-area: number;
}

.list-items .item .content {
  grid-area: content;
  user-select: none;
}

.list-items .item .title {
  margin: 0;
}

.list-items .item .subtitle {
  margin-top: 4px;
  font-size: 0.875rem;
  color: var(--md-sys-color-on-surface-variant);
}

.list-items .item .icon {
  grid-area: icon;
}

.play-title {
  text-align: center;
  padding: 16px;
  white-space: pre-wrap;
  font-size: 0.825rem;
}

.buttons {
  display: flex;
  align-items: center;
  justify-content: space-around;
  padding: 8px;
}

.mode {
  margin: 0px 32px;
}

.audio {
  display: block;
  width: 100%;
}

.playlist-remove-icon {
  color: var(--md-sys-color-error) !important;
}

.item-number {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  background: var(--md-sys-color-surface-container);
  color: var(--md-sys-color-on-surface);
  border-radius: 28px;
  font-weight: 500;
  font-size: 0.875rem;
  cursor: move;
  user-select: none;
  transition: all 0.2s ease;
}

.item-number:hover {
  background: var(--md-sys-color-surface-container-high);
  transform: scale(1.05);
}

.item-number:active {
  cursor: move;
  transform: scale(0.95);
}
</style>
