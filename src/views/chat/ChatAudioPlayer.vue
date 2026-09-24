<template>
  <div class="audio-player" @click.stop>
    <div class="player-row">
      <div class="slider-col">
        <input
          class="slider"
          type="range"
          min="0"
          max="1"
          step="0.001"
          :value="ratio"
          @input="onInput"
          @change="onChange"
        />
        <div class="times">
          <span class="time">{{ formatDurationMs(Math.floor(progressMs)) }}</span>
          <span class="time">{{ formatDurationMs(Math.floor(durationMs)) }}</span>
        </div>
      </div>
      <button class="play-btn" @click="toggle">
        <i-material-symbols:pause-rounded v-if="playing" />
        <i-material-symbols:play-arrow-rounded v-else />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { formatDurationMs } from '@/lib/format'

const props = defineProps<{ src: string }>()

const progressMs = ref(0)
const durationMs = ref(0)
const playing = ref(false)
const ratio = computed(() => durationMs.value > 0 ? progressMs.value / durationMs.value : 0)

let el: HTMLAudioElement | null = null
let dragging = false

onMounted(() => {
  el = new Audio(props.src)
  el.addEventListener('loadedmetadata', () => { durationMs.value = el!.duration * 1000 })
  el.addEventListener('timeupdate', () => { if (!dragging) progressMs.value = el!.currentTime * 1000 })
  el.addEventListener('ended', () => { playing.value = false; progressMs.value = 0 })
  el.play()
  playing.value = true
})

onUnmounted(() => {
  el?.pause()
  el = null
})

function toggle() {
  if (!el) return
  if (playing.value) { el.pause(); playing.value = false }
  else { el.play(); playing.value = true }
}

function onInput(e: Event) {
  dragging = true
  progressMs.value = parseFloat((e.target as HTMLInputElement).value) * durationMs.value
}

function onChange(e: Event) {
  if (el) el.currentTime = parseFloat((e.target as HTMLInputElement).value) * durationMs.value / 1000
  dragging = false
}
</script>

<style lang="scss" scoped>
.audio-player {
  padding: 2px 16px 12px;
}

.player-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.slider-col {
  flex: 1;
  min-width: 0;
}

.slider {
  width: 100%;
  height: 3px;
  accent-color: var(--md-sys-color-on-surface-variant);
  cursor: pointer;
  display: block;
}

.times {
  display: flex;
  justify-content: space-between;
  margin-top: 3px;
}

.time {
  font-size: 0.72rem;
  color: var(--md-sys-color-on-surface-variant);
}

.play-btn {
  flex-shrink: 0;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  border: none;
  background: var(--md-sys-color-surface-container-highest);
  color: var(--md-sys-color-on-surface);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  transition: background 0.15s;

  &:hover {
    background: var(--md-sys-color-surface-variant);
  }
}
</style>
