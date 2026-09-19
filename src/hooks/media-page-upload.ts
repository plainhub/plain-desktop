import { computed, type Ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useI18n } from 'vue-i18n'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import type { IBucket, IFilter } from '@/lib/interfaces'
import { type DataType } from '@/lib/data'
import { useDragDropUpload, useFileUpload } from '@/hooks/upload'
import { createBucketUploadTarget, getDefaultUploadDir } from '@/hooks/media-upload'
import { pickUploadDir } from '@/lib/upload/pick-upload-dir'
import { useMounts } from '@/hooks/files'

export interface MediaPageUploadOptions {
  dataType: DataType
  filter: Pick<IFilter, 'bucketId' | 'trash'>
  buckets: Ref<IBucket[]>
  fileFilter: (name: string) => boolean
  uploadModalId: string
  uploadStorageKey: string
}

export function useMediaPageUpload(options: MediaPageUploadOptions) {
  const mainStore = useMainStore()
  const tempStore = useTempStore()
  const { uploads } = storeToRefs(tempStore)
  const { t } = useI18n()

  const { input: fileInput, upload: uploadFiles, uploadChanged } = useFileUpload(uploads)
  const { input: dirFileInput, upload: doUploadDir, uploadChanged: dirUploadChanged } = useFileUpload(uploads)
  const { dropping, fileDragEnter, fileDragLeave, dropFiles } = useDragDropUpload(uploads)

  const picker = {
    title: t('change_save_location'),
    initialPath: '',
    modalId: options.uploadModalId,
    getValue: () => mainStore.uploadDirs[options.uploadStorageKey] ?? '',
    setValue: (v: string) => { mainStore.uploadDirs = { ...mainStore.uploadDirs, [options.uploadStorageKey]: v } },
  }
  const uploadTarget = createBucketUploadTarget({ filter: options.filter, buckets: options.buckets, picker })

  const { mounts } = useMounts()
  const uploadDir = computed(() => {
    const bucketDir = uploadTarget.getSelectedBucketDir()
    if (bucketDir) return bucketDir
    const internalPath = mounts.value.find((m) => m.driveType === 'INTERNAL_STORAGE')?.path ?? ''
    return mainStore.uploadDirs[options.uploadStorageKey] || getDefaultUploadDir(options.dataType, internalPath)
  })
  const uploadDirEditable = computed(() => !uploadTarget.getSelectedBucketDir())

  async function editUploadDir() {
    await pickUploadDir({ ...picker, initialPath: uploadDir.value })
  }
  async function uploadFilesClick() { const dir = uploadDir.value; if (dir) uploadFiles(dir) }
  async function uploadDirClick() { const dir = uploadDir.value; if (dir) doUploadDir(dir) }
  function dropFiles2(e: DragEvent) { dropFiles(e, () => Promise.resolve(uploadDir.value || undefined), (file) => options.fileFilter(file.name)) }

  return {
    fileInput, dirFileInput, uploadChanged, dirUploadChanged, dropping, fileDragEnter, fileDragLeave,
    uploadFilesClick, uploadDirClick, dropFiles2, uploadDir, uploadDirEditable, editUploadDir,
  }
}
