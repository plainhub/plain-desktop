import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { getUploadUrl } from '@/lib/upload/upload'
import { chachaEncrypt, bitArrayToUint8Array } from '@/lib/api/crypto'
import { tokenToKey } from '@/lib/api/file'
import toast from '@/components/toaster'
import { getCurrentAuthToken } from '@/lib/device/current'
import { get as prefsGet } from '@/lib/prefs'

let uploadGeneration = 0

// Module-level state: persists across modal close/reopen
const uploading = ref(false)
const uploadStatus = ref('')
const uploadProgress = ref(0)
const uploadDone = ref(false)
let currentXhr: XMLHttpRequest | null = null

export function useAIModelUpload() {
  const { t } = useI18n()

  async function startUpload(files: FileList, modelDir: string) {
    if (uploading.value) return
    const generation = ++uploadGeneration
    uploading.value = true
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
    const token = getCurrentAuthToken()
    const key = tokenToKey(token)
    const clientId = prefsGet('client_id', '')
    const totalBytes = valid.reduce((s, f) => s + f.size, 0)
    let completedBytes = 0
    try {
      for (const file of valid) {
        if (generation !== uploadGeneration) throw new Error('Upload cancelled')
        uploadStatus.value = t('ai.uploading', { name: file.name })
        await uploadSingleFile(file, modelDir, key, clientId, (loaded) => {
          uploadProgress.value = Math.min(Math.round(((completedBytes + loaded) / totalBytes) * 100), 99)
        })
        completedBytes += file.size
        uploadProgress.value = Math.round((completedBytes / totalBytes) * 100)
      }
      if (generation !== uploadGeneration) throw new Error('Upload cancelled')
      uploadProgress.value = 100
      uploadStatus.value = t('ai.upload_done')
      uploadDone.value = true
    } catch (e: any) {
      if (e.message !== 'Upload cancelled') {
        toast(t('ai.upload_failed', { error: e.message || 'Unknown error' }), 'error')
      }
    } finally {
      if (generation === uploadGeneration) {
        uploading.value = false
        currentXhr = null
      }
    }
  }

  function cancelUpload() {
    uploadGeneration++
    currentXhr?.abort()
    currentXhr = null
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

function uploadSingleFile(file: File, dir: string, key: Uint8Array, clientId: string, onProgress: (loaded: number) => void): Promise<void> {
  return new Promise((resolve, reject) => {
    const info = JSON.stringify({ dir, replace: true, isAppFile: false, size: file.size })
    const encrypted = bitArrayToUint8Array(chachaEncrypt(key, info))
    const form = new FormData()
    form.append('info', new Blob([encrypted.buffer as ArrayBuffer]))
    form.append('file', file, file.name)
    const xhr = new XMLHttpRequest()
    currentXhr = xhr
    xhr.upload.addEventListener('progress', (e) => {
      if (e.lengthComputable) onProgress(e.loaded)
    })
    xhr.onreadystatechange = () => {
      if (xhr.readyState === 4) {
        currentXhr = null
        if (xhr.status === 201) resolve()
        else reject(new Error(xhr.responseText || `HTTP ${xhr.status}`))
      }
    }
    xhr.onerror = () => {
      currentXhr = null
      reject(new Error('Network error'))
    }
    xhr.ontimeout = () => {
      currentXhr = null
      reject(new Error('Upload timed out'))
    }
    xhr.onabort = () => {
      currentXhr = null
      reject(new Error('Upload cancelled'))
    }
    xhr.open('POST', getUploadUrl(), true)
    xhr.setRequestHeader('c-id', clientId)
    xhr.send(form)
  })
}
