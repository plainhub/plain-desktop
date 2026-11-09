<template>
  <Lightbox popup />
</template>

<script setup lang="ts">
import { onMounted, onBeforeUnmount, watch } from 'vue'
import { useTempStore } from '@/stores/temp'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import type { ISource } from '@/components/lightbox/types'
import { setWindowDeviceName } from '@/lib/api/tauri-window'
import { nativeResourceUrl, closeNativeResource, isNativeResourceUrl } from '@/hooks/native-resource-url'

const tempStore = useTempStore()
const { t } = useI18n()
let active = true
let openedReference = ''

async function parseSource(): Promise<ISource | null> {
  const params = new URLSearchParams(window.location.search)
  let src = params.get('src') ?? ''
  const name = params.get('name') ?? ''
  const path = params.get('path') ?? src
  if (!src && !path) return null
  if (!src || isNativeResourceUrl(src)) {
    if (!path || isNativeResourceUrl(path)) throw new Error('Native preview requires the original file reference')
    openedReference = path
    src = await nativeResourceUrl(path)
    if (!src) throw new Error('Native preview is unavailable in this window')
    if (!active) return null
  }
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
    durationMs: num('durationMs'),
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

onMounted(async () => {
  try {
    const source = await parseSource()
    if (!source || !active) return
    tempStore.lightbox = { sources: [source], index: 0, visible: true, readOnly: false }
    if (inTauri()) await setWindowDeviceName(source.name || 'Preview')
  } catch (error) {
    if (openedReference) await closeNativeResource(openedReference)
    if (active) {
      console.warn('Media preview could not be opened', error)
      toast(t('failed_to_load_file'), 'error')
    }
  }
})

onBeforeUnmount(() => {
  active = false
  if (openedReference) void closeNativeResource(openedReference)
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


