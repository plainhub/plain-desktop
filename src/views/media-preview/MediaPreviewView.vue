<template>
  <Lightbox popup />
</template>

<script setup lang="ts">
import { onMounted, onBeforeUnmount, watch } from 'vue'
import { useTempStore } from '@/stores/temp'
import type { ISource } from '@/components/lightbox/types'
import { setWindowDeviceName } from '@/lib/api/tauri-window'

const tempStore = useTempStore()

function parseSource(): ISource | null {
  const params = new URLSearchParams(window.location.search)
  const src = params.get('src')
  if (!src) return null
  const name = params.get('name') ?? ''
  const path = params.get('path') ?? src
  const num = (k: string) => {
    const v = params.get(k)
    return v == null ? 0 : Number(v)
  }
  const bool = (k: string) => params.get(k) === '1'
  const source: ISource = {
    src,
    path,
    name,
    size: num('size'),
    durationMs: num('duration') * 1000, // legacy param carried seconds; no current sender
    fileId: params.get('fileId') ?? undefined,
    extension: params.get('ext') ?? undefined,
    thumbnail: params.get('thumbnail') ?? undefined,
    viewOriginImage: bool('origin'),
    isFromChat: true,
  }
  return source
}

function inTauri() {
  return __IS_TAURI__
}

onMounted(() => {
  const source = parseSource()
  if (!source) return
  tempStore.lightbox = { sources: [source], index: 0, visible: true, readOnly: false }
  if (inTauri()) {
    setWindowDeviceName(source.name || 'Preview')
  }
})

onBeforeUnmount(() => {
  if (tempStore.lightbox.sources.length === 1) {
    tempStore.lightbox = { sources: [], visible: false, index: -1, readOnly: false }
  }
})

watch(
  () => tempStore.lightbox.visible,
  (visible) => {
    if (!visible && inTauri()) {
      window.close()
    }
  }
)
</script>


