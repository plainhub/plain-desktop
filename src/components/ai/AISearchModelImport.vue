<template>
  <div>
    <template v-if="uploading">
      <progress-card :label-html="uploadStatus" :value="uploadProgress" />
      <v-outlined-button class="btn-block import-action" @click="cancelUpload">{{ $t('ai.cancel_upload') }}</v-outlined-button>
    </template>
    <v-outlined-button v-else class="btn-block" :disabled="enableLoading" @click="fileInput?.click()">{{ $t('ai.manual_upload') }}</v-outlined-button>
    <input ref="fileInput" hidden type="file" multiple accept=".onnx,.json" @change="onFileChange" />
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useAIModelUpload } from '@/hooks/ai-model-upload'
import { useImageSearchActions } from '@/hooks/ai/use-image-search-actions'

const props = defineProps<{ modelDir: string }>()
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
.import-action {
  margin-top: 12px;
}
</style>
