<!-- eslint-disable vue/no-v-html -->
<template>
  <div class="chat-input" :class="{ 'capture-enabled': isTauri }">
    <div class="textarea-wrapper" @dragenter="fileDragEnter" @dragover="fileDragOver" @dragleave="fileDragLeave" @drop="dropFiles">
      <div v-show="displayDragMask" class="drag-mask">{{ $t('release_to_send_files') }}</div>
      <EmojiTextField
        :model-value="modelValue"
        type="textarea"
        :rows="2"
        autocomplete="off"
        class="textarea"
        :placeholder="$t('chat_input_hint')"
        @update:model-value="$emit('update:modelValue', $event)"
        @paste="pasteFiles"
        @keydown="onKeyDown"
        @compositionstart="onCompositionStart"
        @compositionend="onCompositionEnd"
      >
        <template #leading-icon>
          <div class="leading-icons">
            <v-icon-button @click="sendImages">
              <i-material-symbols:image-outline-rounded />
            </v-icon-button>
            <v-icon-button @click="sendFiles">
              <i-material-symbols:folder-outline-rounded />
            </v-icon-button>
            <v-icon-button v-if="isTauri" v-tooltip="$t('screenshot')" data-testid="screen-capture-button" @click="$emit('request-capture')">
              <i-material-symbols:content-cut-rounded />
            </v-icon-button>
          </div>
        </template>
        <template #trailing-icon>
          <v-icon-button class="btn-send" :disabled="createLoading" @click="handleSend">
            <i-material-symbols:send-outline-rounded />
          </v-icon-button>
        </template>
      </EmojiTextField>
    </div>
    <input ref="fileInput" style="display: none" type="file" multiple @change="uploadFilesChanged" />
    <input ref="imageInput" style="display: none" type="file" accept="image/*, video/*" multiple @change="uploadImagesChanged" />
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { isImage, isVideo } from '@/lib/file'

interface Props {
  modelValue: string
  createLoading: boolean
}

interface Emits {
  (e: 'update:modelValue', value: string): void
  (e: 'send-message', message: string): void
  (e: 'send-files', files: File[]): void
  (e: 'send-images', files: File[]): void
  (e: 'request-capture'): void
}

const props = defineProps<Props>()
const emit = defineEmits<Emits>()

const { t } = useI18n()
const isTauri = __IS_TAURI__
const fileInput = ref<HTMLInputElement>()
const imageInput = ref<HTMLInputElement>()
const displayDragMask = ref(false)
const isComposing = ref(false)

function handleSend() {
  emit('send-message', '')
}

function onCompositionStart() {
  isComposing.value = true
}

function onCompositionEnd() {
  isComposing.value = false
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key !== 'Enter') return
  if (e.isComposing || e.keyCode === 229 || isComposing.value) return
  e.preventDefault()
  if (e.shiftKey || e.ctrlKey || e.altKey || e.metaKey) {
    emit('update:modelValue', props.modelValue + '\n')
  } else {
    handleSend()
  }
}

function uploadFilesChanged(e: Event) {
  const files = (e.target as HTMLInputElement).files as FileList
  const items: File[] = []
  for (const item of files) {
    items.push(item)
  }
  emit('send-files', items)
}

function uploadImagesChanged(e: Event) {
  const files = (e.target as HTMLInputElement).files as FileList
  const items: File[] = []
  for (const item of files) {
    items.push(item)
  }
  emit('send-images', items)
}

function sendImages() {
  if (__IS_TAURI__) {
    pickImagesViaTauri()
  } else {
    imageInput.value!.value = ''
    imageInput.value!.click()
  }
}

const IMAGE_MIME: Record<string, string> = {
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  png: 'image/png',
  gif: 'image/gif',
  webp: 'image/webp',
  bmp: 'image/bmp',
  heic: 'image/heic',
  heif: 'image/heif',
  avif: 'image/avif',
  apng: 'image/apng',
  tiff: 'image/tiff',
  tif: 'image/tiff',
  svg: 'image/svg+xml',
}
const VIDEO_MIME: Record<string, string> = {
  mp4: 'video/mp4',
  mov: 'video/quicktime',
  m4v: 'video/x-m4v',
  webm: 'video/webm',
  mkv: 'video/x-matroska',
  avi: 'video/x-msvideo',
  '3gp': 'video/3gpp',
  '3gpp': 'video/3gpp',
}
function mimeFromName(name: string): string {
  const ext = name.toLowerCase().split('.').pop() || ''
  return IMAGE_MIME[ext] || VIDEO_MIME[ext] || ''
}

async function pickImagesViaTauri() {
  const { open } = await import('@tauri-apps/plugin-dialog')
  const { readFile } = await import('@tauri-apps/plugin-fs')
  const selected = await open({
    multiple: true,
    filters: [
      {
        name: 'Media',
        extensions: ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'heic', 'heif', 'avif', 'apng', 'tiff', 'tif', 'svg', 'mp4', 'mov', 'm4v', 'webm', 'mkv', 'avi', '3gp', '3gpp'],
      },
    ],
  })
  if (!selected) return
  const paths = (Array.isArray(selected) ? selected : [selected]) as string[]
  const files: File[] = []
  for (const p of paths) {
    const bytes = await readFile(p)
    const name = p.split('/').pop() || 'file'
    files.push(new File([bytes], name, { type: mimeFromName(name) }))
  }
  if (files.length) {
    emit('send-images', files)
  }
}

function sendFiles() {
  fileInput.value!.value = ''
  fileInput.value!.click()
}

function draggingFiles(e: DragEvent) {
  return !!e.dataTransfer?.types.includes('Files')
}

function fileDragEnter(e: DragEvent) {
  if (draggingFiles(e)) {
    displayDragMask.value = true
  }
}

// dragover must be cancelled for the drop event to fire at all — a textarea
// accepts text drops natively, but not file drops.
function fileDragOver(e: DragEvent) {
  if (draggingFiles(e)) {
    e.preventDefault()
    displayDragMask.value = true
  }
}

function fileDragLeave(e: DragEvent) {
  // dragleave also fires when the pointer crosses into a child element, so
  // only clear the mask once it really is outside the drop area.
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  const inside = e.clientX >= rect.left && e.clientX <= rect.right && e.clientY >= rect.top && e.clientY <= rect.bottom
  if (inside) {
    return
  }
  displayDragMask.value = false
}

// Images and videos become image messages (tappable previews), everything
// else a file message — same split the paste handler has always used.
function sendFilesByKind(files: File[]) {
  const images: File[] = []
  const others: File[] = []
  for (const file of files) {
    const isMedia = file.type.startsWith('image') || file.type.startsWith('video') || isImage(file.name) || isVideo(file.name)
    if (isMedia) {
      images.push(file)
    } else {
      others.push(file)
    }
  }
  if (images.length) {
    emit('send-images', images)
  }
  if (others.length) {
    emit('send-files', others)
  }
  return images.length + others.length > 0
}

function dropFiles(e: DragEvent) {
  displayDragMask.value = false
  const fileList = e.dataTransfer?.files
  if (!fileList?.length) {
    return
  }
  // Only take over the event for files — dropping plain text must still land
  // in the textarea.
  e.preventDefault()
  sendFilesByKind(Array.from(fileList))
}

function pasteFiles(e: ClipboardEvent) {
  const items = e.clipboardData?.items as DataTransferItemList
  if (!items) {
    return
  }
  const files: File[] = []
  for (const item of items) {
    if (item.kind === 'file') {
      files.push(item.getAsFile()!)
    }
  }
  if (sendFilesByKind(files)) {
    e.preventDefault()
  }
}
</script>
