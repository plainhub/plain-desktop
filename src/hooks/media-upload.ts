import type { Ref } from 'vue'
import type { IBucket, IFilter } from '@/lib/interfaces'
import { getDirFromPath } from '@/lib/file'
import { pickUploadDir } from '@/lib/upload/pick-upload-dir'
import { DataType } from '@/lib/data'

const UPLOAD_SUBDIR: Partial<Record<DataType, string>> = {
  [DataType.IMAGE]: 'DCIM',
  [DataType.VIDEO]: 'DCIM',
  [DataType.AUDIO]: 'Music',
  [DataType.DOC]: 'Documents',
}

export function getDefaultUploadDir(dataType: DataType, internalStoragePath: string): string {
  const sub = UPLOAD_SUBDIR[dataType]
  if (!sub) return ''
  const base = internalStoragePath.replace(/\/$/, '')
  return base ? `${base}/${sub}` : ''
}

export function createBucketUploadTarget(options: {
  filter: Pick<IFilter, 'bucketId' | 'trash'>
  buckets: Ref<IBucket[]>
  picker: {
    title: string
    initialPath?: string
    modalId: string
    getValue: () => string
    setValue: (v: string) => void
  }
}) {
  const getSelectedBucketDir = () => {
    const bucketId = options.filter.bucketId
    if (!bucketId) return ''

    const bucket = options.buckets.value.find((it) => it.id === bucketId)
    const top = bucket?.topItemPaths?.[0]
    if (!top) return ''
    return getDirFromPath(top)
  }

  const resolveTargetDir = async (): Promise<string | undefined> => {
    const bucketDir = getSelectedBucketDir()
    if (bucketDir) return bucketDir

    const saved = options.picker.getValue?.()?.trim()
    if (saved) return saved

    return pickUploadDir({
      title: options.picker.title,
      initialPath: options.picker.initialPath || '',
      modalId: options.picker.modalId,
      getValue: options.picker.getValue,
      setValue: options.picker.setValue,
    })
  }

  return {
    getSelectedBucketDir,
    resolveTargetDir,
  }
}
