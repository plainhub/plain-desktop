<template>
  <span class="media-thumbnail" :class="{ loaded, 'scale-down': scaleDown }">
    <canvas v-if="thumbHash && !loaded" ref="preview" aria-hidden="true" />
    <img :key="src" :src="src" loading="lazy" alt="" @load="loadedSrc = src" @error="loadedSrc = null" />
  </span>
</template>

<script setup lang="ts">
import { computed, ref, useTemplateRef, watchEffect } from 'vue'
import { thumbHashPreview } from '@/lib/media/thumb-hash'

const props = defineProps<{ src: string; thumbHash?: string | null; scaleDown?: boolean }>()
const preview = useTemplateRef<HTMLCanvasElement>('preview')
const loadedSrc = ref<string | null>(null)
const loaded = computed(() => loadedSrc.value === props.src)

watchEffect(() => {
  const canvas = preview.value
  if (!canvas || !props.thumbHash) return
  const pixels = thumbHashPreview(props.thumbHash)
  canvas.width = pixels?.width ?? 1
  canvas.height = pixels?.height ?? 1
  if (pixels) canvas.getContext('2d')?.putImageData(pixels, 0, 0)
}, { flush: 'post' })
</script>

<style scoped>
.media-thumbnail {
  display: grid;
  overflow: hidden;
  background-color: var(--md-sys-color-surface-container-highest);
}

.media-thumbnail.loaded {
  background-color: transparent;
}

canvas,
img {
  grid-area: 1 / 1;
  width: 100%;
  height: 100%;
  min-width: 0;
  min-height: 0;
  object-fit: cover;
}

img {
  opacity: 0;
}

.loaded img {
  opacity: 1;
}

.scale-down img {
  object-fit: scale-down;
}
</style>
