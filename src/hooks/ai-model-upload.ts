import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import type { IUploadItem } from '@/stores/temp'
import { addUploadTaskAndWait, removeUpload } from '@/lib/upload/upload-queue'
import { getUploadContext } from '@/lib/upload/context'
import emitter from '@/plugins/eventbus'

let uploadGeneration = 0

// Module-level state: persists across modal close/reopen
const uploading = ref(false)
const uploadStatus = ref('')
const uploadProgress = ref(0)
const uploadDone = ref(false)
let currentUpload: IUploadItem | null = null

export function useAIModelUpload() {
  const { t } = useI18n()

  async function startUpload(files: FileList, modelDir: string) {
    if (uploading.value) return
    const generation = ++uploadGeneration
    uploading.value = true
    const items = new Map(Array.from(files, (file, index) => {
      const item: IUploadItem = { id: `model-${generation}-${index}`, file, fileName: file.name, dir: modelDir, status: 'created', uploadedSize: 0, error: '' }
      getUploadContext(item)
      return [file, item] as const
    }))
    const valid = await validateFiles(files)
    if (generation !== uploadGeneration) return
    if (!valid) {
      uploading.value = false
      toast(t('ai.wrong_files'), 'error')
      return
    }
    uploadDone.value = false
    uploading.value = true
    uploadProgress.value = 0
    const totalBytes = valid.reduce((s, f) => s + f.size, 0)
    let completedBytes = 0
    const onProgress = (item: IUploadItem) => {
      if (generation !== uploadGeneration || item !== currentUpload) return
      uploadProgress.value = Math.min(Math.round(((completedBytes + item.uploadedSize) / totalBytes) * 100), 99)
    }
    emitter.on('upload_progress', onProgress)
    try {
      for (const file of valid) {
        if (generation !== uploadGeneration) throw new Error('Upload canceled')
        uploadStatus.value = t('ai.uploading', { name: file.name })
        currentUpload = items.get(file)!
        await addUploadTaskAndWait(currentUpload, true)
        if (generation !== uploadGeneration) return
        completedBytes += file.size
        uploadProgress.value = Math.round((completedBytes / totalBytes) * 100)
      }
      if (generation !== uploadGeneration) throw new Error('Upload canceled')
      uploadProgress.value = 100
      uploadStatus.value = t('ai.upload_done')
      uploadDone.value = true
    } catch (e: any) {
      if (generation === uploadGeneration) {
        toast(t('ai.upload_failed', { error: e.message || 'Unknown error' }), 'error')
      }
    } finally {
      emitter.off('upload_progress', onProgress)
      if (generation === uploadGeneration) {
        uploading.value = false
        currentUpload = null
      }
    }
  }

  function cancelUpload() {
    uploadGeneration++
    if (currentUpload) removeUpload(currentUpload.id)
    currentUpload = null
    uploading.value = false
    uploadStatus.value = ''
    uploadProgress.value = 0
  }

  return { uploading, uploadStatus, uploadProgress, uploadDone, startUpload, cancelUpload }
}

async function validateFiles(files: FileList): Promise<File[] | null> {
  try {
    const supplied = Array.from(files)
    const declaration = supplied.find((file) => file.name === 'manifest.json')
    if (!declaration || declaration.size > 65536) return null
    const manifest = JSON.parse(await declaration.text())
    if (manifest.formatVersion !== 1 || !Array.isArray(manifest.files) || manifest.files.length < 2 || manifest.files.length > 4) return null
    if (supplied.length !== manifest.files.length + 1 || new Set(supplied.map((file) => file.name)).size !== supplied.length) return null
    const ordered: File[] = []
    const names = new Set<string>()
    for (const asset of manifest.files) {
      if (typeof asset.name !== 'string' || !asset.name || /[\\/\0]/.test(asset.name) || asset.name === 'manifest.json' || names.has(asset.name)) return null
      if (!Number.isSafeInteger(asset.size) || asset.size <= 0 || asset.size > 2147483648 || !/^[0-9a-f]{64}$/i.test(asset.sha256)) return null
      const file = supplied.find((file) => file.name === asset.name && file.size === asset.size)
      if (!file) return null
      ordered.push(file)
      names.add(asset.name)
    }
    if (![manifest.image?.file, manifest.text?.file, manifest.tokenizer?.file].every((name) => names.has(name))) return null
    return [...ordered, declaration]
  } catch {
    return null
  }
}
