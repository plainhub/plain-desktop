import { promptModal } from '@/components/modal'
import DirectoryPickerModal from '@/components/DirectoryPickerModal.vue'
import { get as prefsGet, set as prefsSet } from '@/lib/prefs'

const RECENT_UPLOAD_DIRS_KEY = 'recent_upload_dirs'
const MAX_RECENT_UPLOAD_DIRS = 8

export function getRecentUploadDirs(): string[] {
  return prefsGet<string[]>(RECENT_UPLOAD_DIRS_KEY, []).filter(Boolean)
}

export function addRecentUploadDir(dir: string): void {
  const v = dir.trim()
  if (!v) return
  const list = getRecentUploadDirs().filter((d) => d !== v)
  list.unshift(v)
  prefsSet(RECENT_UPLOAD_DIRS_KEY, list.slice(0, MAX_RECENT_UPLOAD_DIRS))
}

export async function pickUploadDir(options: {
  title?: string
  initialPath?: string
  modalId?: string
  getValue?: () => string
  setValue?: (v: string) => void
}): Promise<string | undefined> {
  const saved = options.getValue ? String(options.getValue() || '').trim() : ''

  const selected = await promptModal<string>(DirectoryPickerModal, {
    title: options.title,
    initialPath: saved || options.initialPath,
    modalId: options.modalId || 'directory-picker',
    recentDirs: getRecentUploadDirs(),
  })

  if (typeof selected !== 'string') return
  const v = selected.trim()
  if (!v) return

  addRecentUploadDir(v)
  options.setValue?.(v)

  return v
}
