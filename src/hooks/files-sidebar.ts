import router, { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { computed, reactive, ref, watch } from 'vue'
import { buildQuery } from '@/lib/search'
import type { IFileFilter, IFavoriteFolder, IStorageMount } from '@/lib/interfaces'
import { useSearch } from '@/hooks/files'
import { decodeBase64, encodeBase64 } from '@/lib/strutil'
import { useI18n } from 'vue-i18n'
import { initMutation, removeFavoriteFolderGQL, setFavoriteFolderAliasGQL } from '@/lib/api/mutation'
import toast from '@/components/toaster'
import emitter from '@/plugins/eventbus'
import { useMounts, useFavoriteFolders } from '@/hooks/files'
import { sortMounts, buildUsbIndexMap, mountTitle as getMountTitle, storageUsedPercent, storageCountText } from '@/lib/storage'
import { openModal } from '@/components/modal'
import EditValueModal from '@/components/EditValueModal.vue'

export interface LinkItem {
  rootPath: string
  fullPath: string
  type: string
  title: string
  isChecked: boolean
  isFavoriteFolder: boolean
  count?: string
  usedPercent?: number
  showProgress?: boolean
}

export function useFilesSidebar() {
  const mainStore = useMainStore()
  const { t } = useI18n()
  const { mounts } = useMounts()
  const { favoriteFolders } = useFavoriteFolders()
  const { parseQ } = useSearch()

  const filter = reactive<IFileFilter>({ showHidden: false, type: '', rootPath: '', text: '', parent: '' })
  const parent = ref('')
  const recent = ref(false)
  const favoriteMenuVisible = ref(false)
  const selectedFavorite = ref<LinkItem | null>(null)

  // Mutations
  const { mutate: removeFavoriteFolderMutation, onDone: onRemoveDone } = initMutation({
    document: removeFavoriteFolderGQL,
  })
  onRemoveDone(() => emitter.emit('refetch_favorite_folders'))

  const { mutate: setAliasMutation, onDone: onAliasDone } = initMutation({
    document: setFavoriteFolderAliasGQL,
  })
  onAliasDone(() => emitter.emit('refetch_favorite_folders'))

  function showFavoriteMenu(item: LinkItem) {
    selectedFavorite.value = item
    const anchorElement = document.getElementById('favorite-' + item.fullPath)
    document.dispatchEvent(new CustomEvent('dropdown-toggle', { detail: { exclude: anchorElement } }))
    favoriteMenuVisible.value = true
  }

  function removeFavoriteFolder(item: LinkItem) {
    removeFavoriteFolderMutation({ fullPath: item.fullPath })
      .then(() => toast(t('removed')))
      .catch(() => toast(t('error'), 'error'))
  }

  function openSetFavoriteAlias() {
    const item = selectedFavorite.value
    if (!item) return
    const current = favoriteFolders.value.find((f) => f.fullPath === item.fullPath)
    const currentAlias = (current?.alias || '').trim()
    const mutationFactory = () => {
      const m = initMutation({ document: setFavoriteFolderAliasGQL })
      m.onDone(() => emitter.emit('refetch_favorite_folders'))
      return m
    }
    openModal(EditValueModal, {
      title: t('name'), placeholder: item.title || '',
      value: currentAlias || '', mutation: mutationFactory,
      getVariables: (value: string) => ({ fullPath: item.fullPath, alias: (value || '').trim() }),
      done: () => toast(t('saved')),
    })
  }

  // Link computation
  function generateFavoriteDisplayTitle(favoriteFolder: IFavoriteFolder): string {
    const alias = (favoriteFolder.alias || '').trim()
    if (alias) return alias
    const usbIndexByMountPoint = buildUsbIndexMap(mounts.value)
    const m = mounts.value.find((it) => it.mountPoint === favoriteFolder.rootPath)
    const rootName = m ? getMountTitle(m, usbIndexByMountPoint, t) : favoriteFolder.rootPath
    const relativePath = favoriteFolder.fullPath.startsWith(favoriteFolder.rootPath)
      ? favoriteFolder.fullPath.substring(favoriteFolder.rootPath.length).replace(/^\//, '')
      : favoriteFolder.fullPath.split('/').pop() || ''
    return relativePath ? `${rootName}/${relativePath}` : rootName
  }

  const links = computed(() => {
    const findLongestMatch = (currentPath: string): string => {
      const allPaths = [
        ...mounts.value.map((m) => m.mountPoint).filter(Boolean),
        ...favoriteFolders.value.map((f) => f.fullPath),
      ]
      let longestMatch = ''
      allPaths.forEach((path) => { if (currentPath.startsWith(path) && path.length > longestMatch.length) longestMatch = path })
      return longestMatch
    }

    const longestMatchPath = recent.value ? '' : findLongestMatch(parent.value)
    const result: LinkItem[] = [{ rootPath: '', fullPath: '', type: 'RECENTS', title: t('recents'), isChecked: recent.value, isFavoriteFolder: false }]

    const usbIndexByMountPoint = buildUsbIndexMap(mounts.value)
    sortMounts(mounts.value).forEach((m) => {
      const mp = m.mountPoint
      if (!mp) return
      const total = Number(m.totalBytes || 0)
      const free = Number(m.freeBytes || 0)
      result.push({
        rootPath: mp, fullPath: mp, type: m.driveType || '',
        title: getMountTitle(m, usbIndexByMountPoint, t),
        isChecked: longestMatchPath === mp, isFavoriteFolder: false,
        count: storageCountText(free, total, t),
        usedPercent: storageUsedPercent(free, total),
        showProgress: total > 0,
      })
    })

    favoriteFolders.value.forEach((folder: IFavoriteFolder) => {
      result.push({
        rootPath: folder.rootPath, fullPath: folder.fullPath,
        type: mounts.value.find((m) => m.mountPoint === folder.rootPath)?.driveType ?? '',
        title: generateFavoriteDisplayTitle(folder),
        isChecked: longestMatchPath === folder.fullPath, isFavoriteFolder: true,
      })
    })

    return result
  })

  const quickLinks = computed(() => links.value.filter((it) => it.type === 'RECENTS'))
  const volumeLinks = computed(() => links.value.filter((it) => it.type !== 'RECENTS' && !it.isFavoriteFolder))
  const favoriteLinks = computed(() => links.value.filter((it) => it.isFavoriteFolder))

  function percentClass(p?: number) {
    return Math.round(p || 0) >= 85 ? 'warn' : ''
  }

  function openLink(link: LinkItem) {
    if (link.type === 'RECENTS') { replacePath(mainStore, '/files/recent'); return }
    const fields: { name: string; op: string; value: string }[] = [
      { name: 'parent', op: '', value: link.fullPath },
      { name: 'type', op: '', value: link.type },
      { name: 'root_path', op: '', value: link.rootPath },
    ]
    if (mainStore.fileShowHidden) fields.push({ name: 'show_hidden', op: '', value: 'true' })
    replacePath(mainStore, `/files?q=${encodeBase64(buildQuery(fields))}`)
  }

  function updateActive() {
    const route = router.currentRoute.value
    if (route.path === '/files/recent') { recent.value = true; return }
    recent.value = false
    const q = decodeBase64(route.query.q?.toString() ?? '')
    parseQ(filter, q)
    parent.value = filter.parent
  }

  updateActive()
  watch(() => router.currentRoute.value.fullPath, () => updateActive())

  return {
    quickLinks, volumeLinks, favoriteLinks,
    favoriteMenuVisible, selectedFavorite,
    openLink, showFavoriteMenu, removeFavoriteFolder, openSetFavoriteAlias, percentClass,
  }
}
