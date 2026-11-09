<template>
  <div class="editor-view">
    <ImageEditor ref="editorRef" />
    <div v-if="loading" class="loading-overlay">
      <v-circular-progress indeterminate />
      <span class="loading-text">{{ loadingText }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import ImageEditor from '@/views/image-editor/components/ImageEditor.vue'
import { nativeResourceImage, isNativeResourceUrl } from '@/hooks/native-resource-url'

const route = useRoute()
const { t } = useI18n()
const editorRef = ref<InstanceType<typeof ImageEditor> | null>(null)
const loading = ref(false)
const loadingText = ref('')
let active = true
const imageRequest = new AbortController()
let imageUrl = ''
const releaseImageUrl = () => {
  if (imageUrl) URL.revokeObjectURL(imageUrl)
  imageUrl = ''
}

onBeforeUnmount(() => {
  active = false
  imageRequest.abort()
  releaseImageUrl()
})

onMounted(async () => {
  const blank = route.query.blank as string | undefined
  const src = route.query.src as string | undefined
  const name = route.query.name as string | undefined
  const reference = route.query.reference as string | undefined
  await nextTick()
  if (!active) return
  if (blank) {
    editorRef.value?.startBlank()
    return
  }
  if (reference) {
    loading.value = true
    try {
      if (isNativeResourceUrl(reference)) throw new Error('Native editing requires the original file reference')
      loadingText.value = 'Loading image…'
      const image = await nativeResourceImage(reference, imageRequest.signal)
      if (active) {
        imageUrl = URL.createObjectURL(image)
        await editorRef.value?.loadImageFromUrl(imageUrl)
      }
    } catch (error) {
      if (active) {
        console.warn('[ImageEditorView] Failed to load native image', error)
        toast(t('failed_to_load_file'), 'error')
      }
    } finally {
      releaseImageUrl()
      loading.value = false
    }
    return
  }
  if (src) {
    if (isNativeResourceUrl(src)) {
      console.warn('[ImageEditorView] Native editing requires the original file reference')
      toast(t('failed_to_load_file'), 'error')
      return
    }
    loading.value = true
    try {
      let fetchUrl = src
      try {
        const u = new URL(src, window.location.origin)
        if (u.origin !== window.location.origin) {
          fetchUrl = u.pathname + u.search
        }
      } catch { /* relative URL — use as-is */ }
      loadingText.value = 'Loading image…'
      const res = await fetch(fetchUrl)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const blob = await res.blob()
      const file = new File([blob], name || 'image', { type: blob.type })
      loadingText.value = 'Opening editor…'
      await editorRef.value?.loadImage(file)
    } catch (e) {
      console.warn('[ImageEditorView] Fetch failed, trying direct URL load', e)
      try {
        await editorRef.value?.loadImageFromUrl(src)
      } catch (e2) {
        console.warn('[ImageEditorView] Failed to load image from URL', e2)
      }
    } finally {
      loading.value = false
    }
  }
})
</script>

<style scoped>
.editor-view {
  position: relative;
  height: 100%;
}

.loading-overlay {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  background: var(--md-sys-color-surface);
}

.loading-text {
  font-size: 14px;
  color: var(--md-sys-color-on-surface-variant);
}
</style>
