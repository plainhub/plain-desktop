import { promptModal } from '@/components/modal'
import DirectoryPickerModal from '@/components/DirectoryPickerModal.vue'
import { readSavedValue, saveValue } from '@/lib/saved-values'

const RECENT_UPLOAD_DIRS_KEY = 'recent_save_dirs'
const MAX_RECENT_UPLOAD_DIRS = 8

export async function getRecentUploadDirs(): Promise<string[]> {
  return (await readSavedValue<string[]>(RECENT_UPLOAD_DIRS_KEY, [])).filter(Boolean)
}

export async function addRecentUploadDir(dir: string): Promise<void> {
  const v = dir.trim()
  if (!v) return
  const list = (await getRecentUploadDirs()).filter((d) => d !== v)
  list.unshift(v)
  await saveValue(RECENT_UPLOAD_DIRS_KEY, list.slice(0, MAX_RECENT_UPLOAD_DIRS))
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
    recentDirs: await getRecentUploadDirs(),
  })

  if (typeof selected !== 'string') return
  const v = selected.trim()
  if (!v) return

  await addRecentUploadDir(v)
  options.setValue?.(v)

  return v
}
