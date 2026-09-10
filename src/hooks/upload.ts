import { ref, type Ref } from 'vue'
import { type IUploadItem } from '@/stores/temp'
import { shortUUID } from '@/lib/strutil'

// shortUUID costs ~30µs per call (randomUUID + per-digit base58 rewrite) —
// 17k directory selections blocked the main thread for half a second just
// generating ids. Upload item ids only need to be unique within this window
// session, so a monotonic counter is enough.
let uploadItemSeq = 0

function createUploadItem(file: File, dir: string, batchId: string, baseDir?: string, relativePath?: string): IUploadItem {
  return {
    id: `upload-${(uploadItemSeq += 1)}`,
    batchId,
    createdAt: new Date().toISOString(),
    dir: dir,
    baseDir,
    relativePath,
    fileName: '',
    file,
    status: 'created',
    uploadedSize: 0,
    error: '',
    pausing: false,
  }
}

function normalizeJoin(base: string, rel: string) {
  const b = (base || '').replace(/\/+$/g, '')
  const r = (rel || '').replace(/^\/+/, '')
  return (b ? `${b}/${r}` : `/${r}`).replace(/\/+/g, '/').replace(/\/+$/g, '')
}

export const useFileUpload = (uploads: Ref<IUploadItem[]>) => {
  let _dir = ''
  const input = ref<HTMLInputElement>()
  return {
    input,
    upload(dir: string) {
      _dir = dir
      input.value!.value = ''
      input.value!.click()
    },
    uploadChanged(e: Event) {
      const files = (e.target as HTMLInputElement).files
      if (!files) {
        return
      }
      const batchId = shortUUID()
      const items = []
      for (let i = 0; i < files.length; i++) {
        const file = files[i]
        const rel = (file as any).webkitRelativePath ? String((file as any).webkitRelativePath) : ''
        if (rel && rel.includes('/')) {
          const parts = rel.split('/').filter(Boolean)
          const relDir = parts.slice(0, -1).join('/')
          const targetDir = relDir ? normalizeJoin(_dir, relDir) : _dir
          items.push(createUploadItem(file, targetDir, batchId, _dir, rel))
        } else {
          items.push(createUploadItem(file, _dir, batchId, _dir, rel || undefined))
        }
      }
      uploads.value = [...uploads.value, ...items]
    },
  }
}

export const useDragDropUpload = (uploads: Ref<IUploadItem[]>) => {
  const dropping = ref(false)

  const readDirectory = async (entry: FileSystemDirectoryEntry, basePath = ''): Promise<Array<{ file: File; relativePath: string }>> => {
    const files: Array<{ file: File; relativePath: string }> = []
    const reader = entry.createReader()

    const readEntries = (): Promise<FileSystemEntry[]> => {
      return new Promise((resolve) => {
        reader.readEntries(resolve)
      })
    }

    const entries = await readEntries()
    for (const entry of entries) {
      if (entry.isFile) {
        const file = await new Promise<File>((resolve) => {
          (entry as FileSystemFileEntry).file(resolve)
        })
        const relativePath = basePath ? `${basePath}/${entry.name}` : entry.name
        files.push({ file, relativePath })
      } else if (entry.isDirectory) {
        const subPath = basePath ? `${basePath}/${entry.name}` : entry.name
        const subFiles = await readDirectory(entry as FileSystemDirectoryEntry, subPath)
        files.push(...subFiles)
      }
    }
    return files
  }

  return {
    dropping,
    fileDragEnter(e: DragEvent) {
      if (e.dataTransfer?.types.includes('Files')) {
        dropping.value = true
      }
    },
    fileDragLeave() {
      dropping.value = false
    },
    async dropFiles(e: DragEvent, dir: string | (() => Promise<string | undefined>), isValid: (file: File) => boolean) {
      dropping.value = false
      const items = e.dataTransfer?.items
      if (!items) {
        return
      }

      const allFileItems: Array<{ file: File; relativePath: string }> = []
      for (let i = 0; i < items.length; i++) {
        const item = items[i]
        if (item.kind === 'file') {
          const entry = item.webkitGetAsEntry()
          if (entry?.isFile) {
            const file = item.getAsFile()
            if (file) allFileItems.push({ file, relativePath: file.name })
          } else if (entry?.isDirectory) {
            const dirFiles = await readDirectory(entry as FileSystemDirectoryEntry, entry.name)
            allFileItems.push(...dirFiles)
          }
        }
      }

      const validFileItems = allFileItems.filter((it) => isValid(it.file))
      if (validFileItems.length === 0) {
        return
      }

      const resolvedDir = typeof dir === 'function' ? await dir() : dir
      const targetBaseDir = String(resolvedDir || '').trim()
      if (!targetBaseDir) {
        return
      }

      const batchId = shortUUID()

      const uploadItems = []
      for (const fileItem of validFileItems) {
        const file = fileItem.file
        const pathParts = fileItem.relativePath.split('/')
        const targetDir = pathParts.length > 1 ? normalizeJoin(targetBaseDir, pathParts.slice(0, -1).join('/')) : targetBaseDir
        uploadItems.push(createUploadItem(file, targetDir, batchId, targetBaseDir, fileItem.relativePath))
      }

      if (uploadItems.length > 0) {
        uploads.value = [...uploads.value, ...uploadItems]
      }
    },
  }
}

export const useChatFilesUpload = () => {
  return {
    getUploads(baseDir: string, files: File[]): IUploadItem[] {
      const batchId = shortUUID()
      const items = []
      for (let i = 0; i < files.length; i++) {
        const file = files[i]
        items.push(createUploadItem(file, baseDir, batchId, baseDir))
      }
      return items
    },
  }
}

export const pasteToUpload = (e: ClipboardEvent, dir: string, uploads: Ref<IUploadItem[]>, type: string = '') => {
  const items = e.clipboardData?.items
  if (!items) {
    return
  }

  const batchId = shortUUID()

  const files: IUploadItem[] = []
  for (const item of items) {
    if (item.kind !== 'file') {
      continue
    }

    const file = item.getAsFile()
    if (file) {
      if (file.type && type && !file.type.startsWith(type)) {
        continue
      }
      files.push(createUploadItem(file, dir, batchId, dir))
    }
  }

  if (files.length > 0) {
    uploads.value = [...uploads.value, ...files]
  }
}
