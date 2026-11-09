import { FileAction } from '@/lib/api/file-action'
import { getRemoteClientId } from '@/lib/device/client-id'
import { isLocalMode } from '@/lib/device/local-mode'
import type { NativeTarget } from '@/lib/api/native-client'
import { onUnmounted, ref, type Ref } from 'vue'
import { createFileTaskGQL, createDirGQL, initMutation, renameFileGQL } from '@/lib/api/mutation'
import type { GqlResultType, GqlVariablesType } from '@/lib/api/graphql/operations'
import { enrichFile, isAudio, isImage, isVideo, type IFile } from '@/lib/file'
import { initQuery, favoriteFoldersGQL, mountsGQL } from '@/lib/api/query'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import { download, downloadAs, encryptUrlParams, getFileId, getFileName, getFileUrl } from '@/lib/api/file'
import type { ISource } from '@/components/lightbox/types'
import { encodeBase64 } from '@/lib/strutil'
import { buildQuery, parseQuery, type IFilterField } from '@/lib/search'
import { arrayRemove } from '@/lib/array'
import { getApiBaseUrl } from '@/lib/api/api'
import type { IApp, IFileFilter, IFavoriteFolder, IStorageMount } from '@/lib/interfaces'
import emitter from '@/plugins/eventbus'

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
  const { t } = useI18n()
  const renameItem = ref<IFile>()
  const renamedPath = ref<string | null>(null)
  return {
    renameItem,
    renamedPath,
    renameDone(newName: string) {
      if (!renamedPath.value) {
        toast(t('rename_failed'), 'error')
        return false
      }
      fetch()
      return true
    },
    renameMutation() {
      renamedPath.value = null
      const mutation = initMutation({ document: renameFileGQL })
      mutation.onDone((result) => { renamedPath.value = result.data?.renameFile ?? null })
      return mutation
    },
    renameVariables(value: string) {
      return { path: renameItem.value?.path, name: value }
    },
  }
}

// Module singleton: every surface (files view, sidebar, media upload
// defaults) shares one `mounts` query and one reactive list.
const storageMounts = ref<IStorageMount[]>([])
let mountsRefetch: (() => void) | undefined

export const useMounts = () => {
  if (!mountsRefetch) {
    const { refetch } = initQuery({
      handle: (data: { mounts: IStorageMount[] }, error: string) => {
        if (!error) {
          storageMounts.value = data?.mounts ?? []
        }
      },
      document: mountsGQL,
    })
    mountsRefetch = refetch
  }

  return { mounts: storageMounts, refetch: mountsRefetch }
}

export const useFavoriteFolders = () => {
  const favoriteFolders = ref<IFavoriteFolder[]>([])
  const { refetch } = initQuery({
    handle: (data: { favoriteFolders: IFavoriteFolder[] }, error: string) => {
      if (!error) {
        favoriteFolders.value = data?.favoriteFolders ?? []
      }
    },
    document: favoriteFoldersGQL,
  })

  const refetchHandler = () => {
    refetch()
  }
  emitter.on('refetch_favorite_folders', refetchHandler)
  onUnmounted(() => emitter.off('refetch_favorite_folders', refetchHandler))

  return { favoriteFolders, refetch }
}

export const useDownload = (urlTokenKey: Ref<Uint8Array | null>) => {
  return {
    async downloadFile(path: string, fileName?: string) {
      const name = fileName || getFileName(path)
      const id = fileName
        ? getFileId(urlTokenKey.value, path)
        : getFileId(urlTokenKey.value, path)
      const url = getFileUrl(id, '', FileAction.Download)
      const target: NativeTarget = isLocalMode() ? { kind: 'local' } : { kind: 'peer', peerId: getRemoteClientId() }
      download(url, name, false, { path, target })
    },
    async downloadFileAs(path: string) {
      const id = getFileId(urlTokenKey.value, path)
      const url = getFileUrl(id, '', FileAction.Download)
      const target: NativeTarget = isLocalMode() ? { kind: 'local' } : { kind: 'peer', peerId: getRemoteClientId() }
      downloadAs(url, getFileName(path), { path, target })
    },
    async downloadDir(path: string, fileName?: string) {
      const name = fileName || `${getFileName(path.replace(/\/+$/, ''))}.zip`
      const id = getFileId(urlTokenKey.value, path)
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

      download(`${getApiBaseUrl()}/zip/files?id=${encodeURIComponent(id)}`, 'download.zip')
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
          src: '',
          name: getFileName(it.path),
          size: it.size,
          updatedAt: it.updatedAt,
          durationMs: 0,
          data: it,
        }))
      const index = sources.value.findIndex((it: ISource) => it.path === f.path)
      ivView(sources.value, index)
    },
  }
}

export const useCopyPaste = (items: Ref<IFile[]>, isCut: Ref<boolean>, selectedFiles: Ref<IFile[]>, refetchFiles: () => void, refetchStats: () => void) => {
  const { mutate, loading } = initMutation({ document: createFileTaskGQL })
  const { t } = useI18n()

  return {
    loading,
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
    async paste(dir: string) {
      const selection = selectedFiles.value
      if (!selection.length) return
      const type = isCut.value ? 'MOVE' : 'COPY'
      const variables: GqlVariablesType<'createFileTaskGQL'> = {
        type, title: type,
        ops: selection.map(file => ({ src: file.path, dst: dir + '/' + file.name, overwrite: false })),
      }
      const result: { data: GqlResultType<'createFileTaskGQL'> } | undefined = await mutate(variables)
      if (result?.data.createFileTask.status === 'ERROR') {
        toast(t(result.data.createFileTask.error))
      } else if (result && selectedFiles.value === selection) {
        selectedFiles.value = []
      }
      refetchFiles()
      refetchStats()
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

export const useDownloadItems = (
  urlTokenKey: Ref<Uint8Array | null>,
  type: string,
  clearSelection: () => void,
  fileName: string | (() => string)
) => {
  const { t } = useI18n()

  return {
    downloadItems: async (realAllChecked: boolean, ids: string[], query: string) => {
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
