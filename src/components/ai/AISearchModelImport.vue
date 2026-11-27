<template>
  <div>
    <template v-if="uploading">
      <progress-card :label-html="uploadStatus" :value="uploadProgress" />
      <v-outlined-button class="btn-block import-action" @click="cancelUpload">{{ $t('ai.cancel_upload') }}</v-outlined-button>
    </template>
    <template v-else>
      <v-outlined-button class="btn-block" :disabled="enableLoading" @click="fileInput?.click()">{{ $t('ai.manual_upload') }}</v-outlined-button>
      <i18n-t keypath="ai.manual_upload_hint" scope="global" tag="p" class="import-hint">
        <template #files><code>{{ siglipFiles }}</code></template>
        <template #manifest><code>{{ manifestFile }}</code></template>
      </i18n-t>
    </template>
    <input ref="fileInput" hidden type="file" multiple accept=".onnx,.json" @change="onFileChange" />
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useAIModelUpload } from '@/hooks/ai-model-upload'
import { useImageSearchActions } from '@/hooks/ai/use-image-search-actions'

const props = defineProps<{ modelDir: string }>()
const manifestFile = 'manifest.json'
const siglipFiles = [manifestFile, 'image.onnx', 'text.onnx', 'tokenizer.json'].join(', ')
const fileInput = ref<HTMLInputElement | null>(null)
const { uploading, uploadStatus, uploadProgress, uploadDone, startUpload, cancelUpload } = useAIModelUpload()
const { importModel, enableLoading } = useImageSearchActions()
watch(
  uploadDone,
  (done) => {
    if (done) {
      uploadDone.value = false
      importModel()
    }
  },
  { immediate: true }
)
function onFileChange(event: Event) {
  const input = event.target as HTMLInputElement
  if (input.files?.length) startUpload(input.files, props.modelDir)
  input.value = ''
}
</script>

<style scoped>
.import-hint {
  margin: 8px 0 0;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.8rem;
  line-height: 1.5;
  overflow-wrap: anywhere;
}

.import-action {
  margin-top: 12px;
}
</style>
