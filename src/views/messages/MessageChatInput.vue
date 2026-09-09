<template>
  <div class="chat-input sms-chat-input" :class="{ 'capture-enabled': isTauri }">
    <div v-if="pendingFiles.length" class="chat-attachment-preview">
      <div v-for="(file, idx) in pendingFiles" :key="idx" class="chat-attachment-preview-item">
        <img v-if="file.type.startsWith('image/')" :src="filePreviewUrl(file)" class="chat-preview-thumb" />
        <div v-else class="chat-preview-file">
          <i-material-symbols:attach-file-rounded />
        </div>
        <span class="chat-preview-name">{{ file.name }}</span>
        <span class="chat-preview-size" :class="{ warn: !file.type.startsWith('image/') && file.size > warnSize }">{{ formatFileSize(file.size) }}</span>
        <v-icon-button class="chat-preview-remove" @click="$emit('removeFile', idx)">
          <i-material-symbols:close-rounded />
        </v-icon-button>
      </div>
      <div v-if="hasLargeNonImageFile" class="chat-size-warning">
        <i-material-symbols:warning-outline-rounded />
        {{ $t('mms_large_file_warning') }}
      </div>
      <div v-else-if="totalPendingSize > warnSize" class="chat-size-hint">
        {{ $t('mms_image_auto_compress') }}
      </div>
    </div>
    <div class="textarea-wrapper">
      <EmojiTextField
        :model-value="modelValue"
        type="textarea"
        :rows="1"
        :placeholder="$t('write_a_message')"
        class="chat-input-field"
        @update:model-value="$emit('update:modelValue', $event)"
        @keydown.enter.exact="onEnterKey"
      >
        <template #leading-icon>
          <div class="leading-icons">
            <v-icon-button v-tooltip="$t('attachments')" @click="$emit('openFilePicker')">
              <i-material-symbols:attach-file-rounded />
            </v-icon-button>
            <v-icon-button v-if="isTauri" v-tooltip="$t('screenshot')" data-testid="sms-screen-capture-button" :disabled="captureDisabled" @click="$emit('requestCapture')">
              <i-material-symbols:content-cut-rounded />
            </v-icon-button>
            <SimSelector v-if="sims.length > 1" :model-value="selectedSimId" :sims="sims" @update:model-value="$emit('update:selectedSimId', $event)" />
          </div>
        </template>
        <template #trailing-icon>
          <v-icon-button class="btn-send" :disabled="sendDisabled" @click="$emit('send')">
            <i-material-symbols:send-outline-rounded />
          </v-icon-button>
        </template>
      </EmojiTextField>
      <input ref="fileInputRef" type="file" multiple accept="image/*,video/*,audio/*" class="hidden-file-input" @change="$emit('fileSelected', $event)" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { formatFileSize } from '@/lib/format'
import type { ISim } from '@/lib/interfaces'
import SimSelector from './SimSelector.vue'

const props = defineProps<{
  modelValue: string
  pendingFiles: File[]
  totalPendingSize: number
  hasLargeNonImageFile: boolean
  warnSize: number
  sendDisabled: boolean
  captureDisabled: boolean
  sims: ISim[]
  selectedSimId: number
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string]
  'update:selectedSimId': [value: number]
  send: []
  openFilePicker: []
  fileSelected: [event: Event]
  removeFile: [index: number]
  requestCapture: []
}>()

const isTauri = __IS_TAURI__
const fileInputRef = defineModel<HTMLInputElement | undefined>('fileInputRef')

function onEnterKey(e: KeyboardEvent) {
  if (e.isComposing || props.sendDisabled) return
  e.preventDefault()
  emit('send')
}

function filePreviewUrl(file: File): string {
  return URL.createObjectURL(file)
}
</script>
