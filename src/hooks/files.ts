import { ref, type Ref } from 'vue'
import { copyFileGQL, createDirGQL, initMutation, moveFileGQL, renameFileGQL } from '@/lib/api/mutation'
import { enrichFile, isAudio, isImage, isVideo, type IFile } from '@/lib/file'
import { initQuery, mountsGQL } from '@/lib/api/query'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import { download, encryptUrlParams, getFileId, getFileName, getFileUrl } from '@/lib/api/file'
import type { ISource } from '@/components/lightbox/types'
import { encodeBase64 } from '@/lib/strutil'
import { buildQuery, parseQuery, type IFilterField } from '@/lib/search'
import { arrayRemove } from '@/lib/array'
import { getApiBaseUrl } from '@/lib/api/api'
import type { IApp, IFileFilter, IStorageMount } from '@/lib/interfaces'

export const useCreateDir = (urlTokenKey: Ref<Uint8Array | null>, items: Ref<IFile[]>) => {
  const createPath = ref('')

  return {
    createPath,
    createVariables(value: string) {
      return { path: createPath.value + '/' + value }
    },
    createMutation() {
      const m = initMutation({ document: createDirGQL })
      m.onDone((r: any) => {
        const d = r?.data?.createDir
        if (d) {
          arrayRemove(items.value, (it: IFile) => it.path === d.path)
          items.value.unshift(enrichFile(d))
        }
      })
      return m
    },
  }
}

export const useRename = (fetch: () => void) => {
  const renameItem = ref<IFile>()
  return {
    renameItem,
    renameDone(newName: string) {
      fetch()
    },
    renameMutation() {
      return initMutation({
        document: renameFileGQL,
      })
    },
    renameVariables(value: string) {
      return { path: renameItem.value?.path, name: value }
    },
  }
}

export const useMounts = () => {
  const mounts = ref<IStorageMount[]>([])
  const { refetch } = initQuery({
    handle: (data: { mounts: IStorageMount[] }, error: string) => {
      if (!error) {
        mounts.value = data?.mounts ?? []
      }
    },
    document: mountsGQL,
  })

  return { mounts, refetch }
}

export const useDownload = (urlTokenKey: Ref<Uint8Array | null>) => {
  return {
    async downloadFile(path: string, fileName?: string) {
      const name = fileName || getFileName(path)
      const id = fileName
        ? getFileId(urlTokenKey.value, JSON.stringify({ path, name: fileName }))
        : getFileId(urlTokenKey.value, path)
      const url = `${getApiBaseUrl()}/fs?id=${encodeURIComponent(id)}&dl=1`
      download(url, name)
    },
    async downloadDir(path: string, fileName?: string) {
      const name = fileName || `${getFileName(path.replace(/\/+$/, ''))}.zip`
      const id = getFileId(urlTokenKey.value, JSON.stringify({ path, name }))
      const url = `${getApiBaseUrl()}/zip/dir?id=${encodeURIComponent(id)}`
      download(url, name)
    },
    downloadFiles(key: string) {
      const id = encryptUrlParams(
        urlTokenKey.value,
        JSON.stringify({
          id: key,
          type: 'FILE',
          name: '',
        })
      )

      download(`${getApiBaseUrl()}/zip/files?id=${encodeURIComponent(id)}`, '')
    },
  }
}

export const useView = (sources: Ref<ISource[]>, ivView: (sources: ISource[], i: number) => void) => {
  return {
    view(items: IFile[], f: IFile) {
      sources.value = items
        .filter((it) => isImage(it.name) || isVideo(it.name) || isAudio(it.name))
        .map((it) => ({
          path: it.path,
          src: it.fileId ? getFileUrl(it.fileId) : '',
          name: getFileName(it.path),
          size: it.size,
          duration: 0,
          data: f,
        }))
      const index = sources.value.findIndex((it: ISource) => it.path === f.path)
      ivView(sources.value, index)
    },
  }
}

export const useCopyPaste = (items: Ref<IFile[]>, isCut: Ref<boolean>, selectedFiles: Ref<IFile[]>, refetchFiles: () => void, refetchStats: () => void) => {
  const dstDir = ref<string>()

  const {
    mutate: copyMutate,
    loading: copyLoading,
    onDone: copyDone,
    onError: copyError,
  } = initMutation({
    document: copyFileGQL,
  })

  const {
    mutate: cutMutate,
    loading: cutLoading,
    onDone: cutDone,
    onError: cutError,
  } = initMutation({
    document: moveFileGQL,
  })

  const { t } = useI18n()

  const onError = (error: any) => {
    toast(t(error?.message ?? 'error'))
  }

  copyError(onError)
  cutError(onError)

  const onDone = () => {
    if (isCut.value) {
      for (const file of selectedFiles.value) {
        arrayRemove(items.value, (it: IFile) => it.path === file.path)
      }
    }

    selectedFiles.value = []

    // have to delay 1s to make sure the api return latest data.
    setTimeout(() => {
      refetchFiles()
      refetchStats()
    }, 500)
  }

  copyDone(onDone)
  cutDone(onDone)

  return {
    loading: copyLoading || cutLoading,
    canPaste() {
      return selectedFiles.value.length > 0
    },
    copy(ids: string[]) {
      selectedFiles.value = items.value.filter((it) => ids.includes(it.id))
      isCut.value = false
    },
    cut(ids: string[]) {
      selectedFiles.value = items.value.filter((it) => ids.includes(it.id))
      isCut.value = true
    },
    paste(dir: string) {
      dstDir.value = dir
      for (const file of selectedFiles.value) {
        const a = {
          src: file.path,
          dst: dir + '/' + file.name,
          overwrite: false,
        }
        if (isCut.value) {
          cutMutate(a)
        } else {
          copyMutate(a)
        }
      }
    },
  }
}

export function getFileDir(fileName: string) {
  let dir = 'Documents'
  if (isImage(fileName)) {
    dir = 'Pictures'
  } else if (isVideo(fileName)) {
    dir = 'Movies'
  } else if (isAudio(fileName)) {
    dir = 'Music'
  }
  return dir
}

export const useDownloadItems = (urlTokenKey: Ref<Uint8Array | null>, type: string, clearSelection: () => void, fileName: string | (() => string)) => {
  const { t } = useI18n()

  return {
    downloadItems: (realAllChecked: boolean, ids: string[], query: string) => {
      let q = query
      if (!realAllChecked) {
        if (ids.length === 0) {
          toast(t('select_first'), 'error')
          return
        }
        q = `ids:${ids.join(',')}`
      }

      // Generate dynamic filename if function is provided
      const finalFileName = typeof fileName === 'function' ? fileName() : fileName

      const id = encryptUrlParams(
        urlTokenKey.value,
        JSON.stringify({
          query: q,
          type: type,
          name: finalFileName,
        })
      )
      download(`${getApiBaseUrl()}/zip/files?id=${encodeURIComponent(id)}`, finalFileName)
      clearSelection()
    },
  }
}

export const useSearch = () => {
  return {
    parseQ: (filter: IFileFilter, q: string) => {
      const fields = parseQuery(q)
      filter.showHidden = false
      filter.text = ''
      filter.parent = ''
      filter.rootPath = ''
      filter.fileSize = undefined
      fields.forEach((it) => {
        if (it.name === 'text') {
          filter.text = it.value
        } else if (it.name === 'parent') {
          filter.parent = it.value
        } else if (it.name === 'type') {
          filter.type = it.value
        } else if (it.name === 'root_path') {
          filter.rootPath = it.value
        } else if (it.name === 'show_hidden') {
          filter.showHidden = it.value === 'true'
        } else if (it.name === 'file_size') {
          filter.fileSize = it.op + it.value
        }
      })
    },
    buildQ: (filter: IFileFilter): string => {
      const fields: IFilterField[] = []
      if (filter.parent !== '') {
        fields.push({
          name: 'parent',
          op: '',
          value: filter.parent,
        })
      }

      if (filter.type !== '') {
        fields.push({
          name: 'type',
          op: '',
          value: filter.type,
        })
      }

      if (filter.text !== '') {
        fields.push({
          name: 'text',
          op: '',
          value: filter.text,
        })
      }

      if (filter.rootPath !== '') {
        fields.push({
          name: 'root_path',
          op: '',
          value: filter.rootPath,
        })
      }

      if (filter.showHidden) {
        fields.push({
          name: 'show_hidden',
          op: '',
          value: filter.showHidden ? 'true' : 'false',
        })
      }

      if (filter.fileSize !== undefined && filter.fileSize !== '') {
        const match = filter.fileSize.match(/^([><=!]+)?(.+)$/)
        if (match) {
          const op = match[1] || ''
          const value = match[2]
          fields.push({
            name: 'file_size',
            op: op,
            value: value,
          })
        }
      }

      return encodeBase64(buildQuery(fields))
    },
  }
}
