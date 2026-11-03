import { computed, ref, watch } from 'vue'
import toast from '@/components/toaster'
import { useI18n } from 'vue-i18n'
import { filesGQL, initLazyQuery, initQuery, mountsGQL } from '@/lib/api/query'
import type { IStorageMount } from '@/lib/interfaces'
import type { IFile } from '@/lib/file'
import { getFileName } from '@/lib/api/file'
import { compareLocale } from '@/lib/array'

export function useDirectoryPicker(initialPath: string) {
  const { t } = useI18n()

  const volumes = ref<IStorageMount[]>([])
  const loadingMounts = ref(true)
  const rootPath = ref('')
  const relativePath = ref('')
  const dirItems = ref<Pick<IFile, 'path' | 'isDir' | 'isSystem'>[]>([])

  const normalizedInitialPath = computed(() => {
    const p = String(initialPath || '').trim()
    return p ? p.replace(/\/+$/g, '') || '/' : ''
  })

  const currentDir = computed(() => {
    const root = rootPath.value || '/'
    const rel = (relativePath.value || '').replace(/^\/+/, '')
    if (!rel) return root
    if (root === '/') return `/${rel}`.replace(/\/+/g, '/').replace(/\/+$/g, '')
    return `${root}/${rel}`.replace(/\/+/g, '/').replace(/\/+$/g, '')
  })

  const canGoUp = computed(() => !!rootPath.value && (relativePath.value || '').trim() !== '')

  function selectRoot(mountPoint: string) { rootPath.value = mountPoint || '/'; relativePath.value = '' }

  function normalizeRelativeFromAbs(absPath: string) {
    const root = rootPath.value || ''
    if (!root) return ''
    if (root === '/') return absPath.replace(/^\/+/, '')
    if (!absPath.startsWith(root)) return ''
    return absPath.slice(root.length).replace(/^\/+/, '')
  }

  function enterDir(absPath: string) { relativePath.value = normalizeRelativeFromAbs(absPath) }

  function goUp() {
    const rel = (relativePath.value || '').replace(/\/+$/g, '')
    const idx = rel.lastIndexOf('/')
    relativePath.value = idx <= 0 ? '' : rel.slice(0, idx)
  }

  function dirName(path: string) { return getFileName(path) || path }

  initQuery<{ mounts: IStorageMount[] }>({
    document: mountsGQL,
    handle: (data, error) => {
      loadingMounts.value = false
      if (error) { toast(t(error), 'error'); return }
      volumes.value = [...(data?.mounts ?? [])].sort((a, b) => {
        const am = String(a?.mountPoint ?? ''), bm = String(b?.mountPoint ?? '')
        if (am === '/') return -1; if (bm === '/') return 1
        return compareLocale(am, bm, { numeric: true })
      })
      if (!rootPath.value) { rootPath.value = volumes.value[0]?.mountPoint || '/'; relativePath.value = '' }

      const init = normalizedInitialPath.value
      if (init) {
        const mountPoints = volumes.value.map((v) => String(v.mountPoint || '').trim()).filter(Boolean)
        let best = ''
        for (const m of mountPoints) {
          if (m === '/' && init.startsWith('/')) { if (best.length < 1) best = '/'; continue }
          if (m !== '/' && (init === m || init.startsWith(m + '/'))) { if (m.length > best.length) best = m }
        }
        if (best) {
          rootPath.value = best
          relativePath.value = best === '/' ? init.replace(/^\/+/, '') : init.slice(best.length).replace(/^\/+/, '')
        }
      }
    },
  })

  const { loading: listing, fetch: fetchDirs } = initLazyQuery<{ files: Pick<IFile, 'path' | 'isDir' | 'isSystem'>[] }>({
    document: filesGQL,
    variables: () => ({ root: currentDir.value, offset: 0, limit: 10000, query: '', sortBy: 'NAME_ASC' }),
    handle: (data, error) => {
      if (error) { dirItems.value = []; toast(t(error), 'error'); return }
      dirItems.value = (data?.files ?? []).filter((f) => f.isDir)
    },
  })

  watch([rootPath, relativePath], () => { if (rootPath.value) fetchDirs() }, { immediate: true })

  return { volumes, loadingMounts, rootPath, currentDir, canGoUp, listing, dirItems, selectRoot, enterDir, goUp, dirName }
}
