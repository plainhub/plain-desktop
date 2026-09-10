import { onActivated, onDeactivated, watch, type Ref } from 'vue'
import emitter from '@/plugins/eventbus'
import type { IUploadItem } from '@/stores/temp'
import type { IFile } from '@/lib/file'
import type { IFileDeletedEvent, IFileRenamedEvent } from '@/lib/interfaces'
import { createUploadRefreshScheduler } from '@/lib/upload/refresh-scheduler'

interface UseFilesEventsOptions {
  isActive: Ref<boolean>
  fileSortBy: Ref<string>
  routeFullPath: () => string
  applyRouteQuery: () => void
  fetch: (vars?: Record<string, any>, options?: { latest?: boolean }) => void
  refetchMounts: () => void
  refreshing: Ref<boolean>
  sorting: Ref<boolean>
  onDeleted: (files: IFile[]) => void
  pageKeyDown: (e: KeyboardEvent) => void
  pageKeyUp: (e: KeyboardEvent) => void
  parentDir: () => string
}

function isUnderDir(path: string, parent: string) {
  const p = path.replace(/\/+$/, '')
  const b = parent.replace(/\/+$/, '')
  return !b || p === b || p.startsWith(b + '/')
}

export function useFilesEvents(opts: UseFilesEventsOptions) {
  const {
    isActive, fileSortBy, routeFullPath, applyRouteQuery, fetch, refetchMounts,
    refreshing, sorting, onDeleted, pageKeyDown, pageKeyUp, parentDir,
  } = opts

  // Refreshing once per completed file is quadratic for large directory
  // uploads — coalesce into a rare refresh and drop stale responses.
  const uploadRefresh = createUploadRefreshScheduler(() => {
    fetch(undefined, { latest: true })
    refetchMounts()
  })
  const uploadTaskDoneHandler = (r: IUploadItem) => {
    if (r.status === 'done' && isUnderDir(r.dir, parentDir())) uploadRefresh.schedule()
  }

  const fileDeletedHandler = (event: IFileDeletedEvent) => { onDeleted([event.item]) }
  const fileRenamedHandler = (_: IFileRenamedEvent) => { fetch() }

  watch(routeFullPath, () => {
    if (!isActive.value) return
    applyRouteQuery()
    fetch()
  })

  watch(fileSortBy, () => {
    sorting.value = true
    fetch()
  })

  onActivated(() => {
    isActive.value = true
    applyRouteQuery()
    fetch()
    emitter.on('upload_task_done', uploadTaskDoneHandler)
    emitter.on('file_deleted', fileDeletedHandler)
    emitter.on('file_renamed', fileRenamedHandler)
    window.addEventListener('keydown', pageKeyDown)
    window.addEventListener('keyup', pageKeyUp)
  })

  onDeactivated(() => {
    isActive.value = false
    uploadRefresh.dispose()
    emitter.off('upload_task_done', uploadTaskDoneHandler)
    emitter.off('file_deleted', fileDeletedHandler)
    emitter.off('file_renamed', fileRenamedHandler)
    window.removeEventListener('keydown', pageKeyDown)
    window.removeEventListener('keyup', pageKeyUp)
  })
}
