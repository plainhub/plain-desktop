<!-- eslint-disable vue/no-v-html -->
<template>
  <div class="chat-input">
    <div class="textarea-wrapper" @dragenter="fileDragEnter" @dragover="fileDragOver" @dragleave="fileDragLeave" @drop="dropFiles">
      <div v-show="displayDragMask" class="drag-mask">{{ $t('release_to_send_files') }}</div>
      <div v-if="attachments?.length" class="staged-attachments">
        <ChatDraftAttachment v-for="(attachment, index) of attachments" :key="index" :attachment="attachment" :disabled="createLoading" @remove="$emit('remove-attachment', index)" />
      </div>
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
        <template #footer>
          <div class="input-toolbar">
            <div class="toolbar-actions">
              <v-icon-button @click="sendImages">
                <i-material-symbols:image-outline-rounded />
              </v-icon-button>
              <v-icon-button @click="sendFiles">
                <i-material-symbols:folder-outline-rounded />
              </v-icon-button>
              <div v-if="isTauri" class="capture-split">
                <button
                  v-tooltip="captureTooltip"
                  type="button"
                  class="btn-icon capture-main"
                  data-testid="screen-capture-button"
                  @click="$emit('request-capture')"
                >
                  <i-material-symbols:content-cut-rounded />
                  <span
                    v-if="shortcutStatus && !shortcutStatus.registered"
                    class="capture-error-badge"
                    data-testid="capture-shortcut-error-badge"
                  >!</span>
                </button>
                <v-dropdown v-model="captureMenuOpen">
                  <template #trigger>
                    <button
                      type="button"
                      class="btn-icon capture-arrow"
                      :aria-label="$t('screen_capture.ui.menuA11y')"
                      data-testid="screen-capture-menu-button"
                    >
                      <i-material-symbols:keyboard-arrow-down-rounded />
                    </button>
                  </template>
                  <div class="capture-input-menu">
                    <button type="button" class="capture-input-menu-item" data-testid="capture-menu-capture" @click="menuCapture">
                      <i-material-symbols:content-cut-rounded />
                      <span>{{ $t('screenshot') }}</span>
                      <span v-if="shortcutStatus?.accelerator" class="capture-input-menu-hint">{{ formatAccelerator(shortcutStatus.accelerator, isMacPlatform()) }}</span>
                    </button>
                    <button type="button" class="capture-input-menu-item" data-testid="capture-menu-shortcut" @click="openShortcutSettings">
                      <i-material-symbols:keyboard-outline-rounded />
                      <span>{{ $t('screen_capture.ui.shortcutSettings') }}</span>
                    </button>
                    <button
                      v-if="shortcutStatus && !shortcutStatus.registered"
                      type="button"
                      class="capture-input-menu-item capture-input-menu-danger"
                      data-testid="capture-menu-permission"
                      @click="openPermissionSettings"
                    >
                      <i-material-symbols:error-outline-rounded />
                      <span>{{ $t('screen_capture.ui.openPermissionSettings') }}</span>
                    </button>
                  </div>
                </v-dropdown>
              </div>
            </div>
            <button type="button" class="format-toggle" :aria-pressed="format === 'MARKDOWN'" :disabled="createLoading" @click="$emit('toggle-format')">Markdown</button>
            <v-icon-button class="toolbar-send" :disabled="createLoading" @click="handleSend">
              <i-material-symbols:send-outline-rounded />
            </v-icon-button>
          </div>
        </template>
      </EmojiTextField>
    </div>
    <input ref="fileInput" style="display: none" type="file" multiple @change="uploadFilesChanged" />
    <input ref="imageInput" style="display: none" type="file" accept="image/*, video/*" multiple @change="uploadImagesChanged" />
    <CaptureShortcutModal :open="shortcutModalOpen" :status="shortcutStatus" @update:open="shortcutModalOpen = $event" @saved="onShortcutSaved" />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ChatTextFormat, DraftAttachment } from '@/lib/chat/message-draft'
import { isImage, isVideo } from '@/lib/file'
import type { CaptureShortcutStatus } from '@/lib/screen-capture/capture-shortcut'
import { formatAccelerator } from '@/lib/screen-capture/shortcut-recorder'
import { isMacPlatform } from '@/lib/platform'
import ChatDraftAttachment from './components/ChatDraftAttachment.vue'
import CaptureShortcutModal from './components/CaptureShortcutModal.vue'

interface Props {
  modelValue: string
  createLoading: boolean
  attachments?: readonly DraftAttachment[]
  format?: ChatTextFormat
}

interface Emits {
  (e: 'update:modelValue', value: string): void
  (e: 'send-message', message: string): void
  (e: 'stage-attachments', items: DraftAttachment[]): void
  (e: 'remove-attachment', index: number): void
  (e: 'toggle-format'): void
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
const captureMenuOpen = ref(false)
const shortcutModalOpen = ref(false)
const shortcutStatus = ref<CaptureShortcutStatus | null>(null)

const captureTooltip = computed(() => {
  const accelerator = shortcutStatus.value?.accelerator
  if (!accelerator) return t('screenshot')
  return `${t('screenshot')} ${formatAccelerator(accelerator, isMacPlatform())}`
})

onMounted(() => {
  if (!isTauri) return
  void import('@/lib/screen-capture/capture-shortcut')
    .then(({ getCaptureShortcutStatus }) => getCaptureShortcutStatus())
    .then((status) => {
      shortcutStatus.value = status
    })
    .catch(() => {})
})

function menuCapture() {
  captureMenuOpen.value = false
  emit('request-capture')
}

function openShortcutSettings() {
  captureMenuOpen.value = false
  shortcutModalOpen.value = true
}

function openPermissionSettings() {
  captureMenuOpen.value = false
  void import('@/lib/screen-capture/capture-permission')
    .then((module) => module.openScreenCapturePermissionSettings())
    .catch(() => {})
}

function onShortcutSaved(status: CaptureShortcutStatus) {
  shortcutStatus.value = status
}

function handleSend() {
  if (!props.createLoading && (props.modelValue.trim() || props.attachments?.length)) emit('send-message', props.modelValue)
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
  emit('stage-attachments', items.map(file => ({ type: 'FILE', file })))
}

function uploadImagesChanged(e: Event) {
  const files = (e.target as HTMLInputElement).files as FileList
  const items: File[] = []
  for (const item of files) {
    items.push(item)
  }
  emit('stage-attachments', items.map(file => ({ type: 'IMAGE', file })))
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
  const { open } = await import('@/lib/tauri-plugins')
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
    emit('stage-attachments', files.map(file => ({ type: 'IMAGE', file })))
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

// dragover must be canceled for the drop event to fire at all — a textarea
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
  if (!files.length) return false
  emit('stage-attachments', files.map(file => ({
    type: file.type.startsWith('image/') || file.type.startsWith('video/') || isImage(file.name) || isVideo(file.name) ? 'IMAGE' : 'FILE',
    file,
  })))
  return true
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
    const file = item.kind === 'file' ? item.getAsFile() : null
    if (file) files.push(file)
  }
  if (sendFilesByKind(files)) {
    e.preventDefault()
  }
}
</script>

<style lang="scss" scoped>
.staged-attachments { display: flex; flex-wrap: wrap; gap: 6px; padding: 8px 0; }
.format-toggle { margin-left: auto; border: 0; border-radius: 8px; padding: 4px 8px; color: var(--md-sys-color-on-surface-variant); background: transparent; }
.format-toggle[aria-pressed="true"] { background: var(--md-sys-color-secondary-container); color: var(--md-sys-color-on-secondary-container); }

.capture-split {
  display: inline-flex;
  align-items: center;
  position: relative;
  width: 32px;
  height: 32px;
  border-radius: 8px;

  &:hover {
    background: rgba(0, 0, 0, 0.08);
  }
}

.capture-split .btn-icon {
  width: auto;
  height: 32px;
  border-radius: 0;
  background: none;

  &:hover:not(:disabled) {
    background: none;
  }
}

.capture-main {
  padding: 0 2px 0 6px;

  svg {
    width: 18px;
    height: 18px;
  }
}

.capture-arrow {
  padding: 0 4px 0 0;

  svg {
    width: 14px;
    height: 14px;
  }
}

.capture-error-badge {
  position: absolute;
  top: 0;
  right: 0;
  width: 12px;
  height: 12px;
  border-radius: 999px;
  background: var(--md-sys-color-error);
  color: var(--md-sys-color-on-error);
  font-size: 9px;
  font-weight: 700;
  line-height: 12px;
  text-align: center;
  pointer-events: none;
}
</style>

<style lang="scss">
/* Menu content teleports to <body>, so these styles cannot be scoped. */
.capture-input-menu {
  display: flex;
  flex-direction: column;
  min-width: 220px;
  padding: 4px;
}

.capture-input-menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  border: none;
  background: none;
  border-radius: 8px;
  padding: 8px 12px;
  cursor: pointer;
  font-family: inherit;
  font-size: 0.875rem;
  color: var(--md-sys-color-on-surface);
  text-align: start;

  svg {
    width: 18px;
    height: 18px;
    flex-shrink: 0;
  }

  &:hover {
    background: var(--md-sys-color-surface-variant);
  }
}

.capture-input-menu-hint {
  margin-inline-start: auto;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.75rem;
}

.capture-input-menu-danger {
  color: var(--md-sys-color-error);
}
</style>
