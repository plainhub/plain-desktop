import { promptModal } from '@/components/modal'
import DirectoryPickerModal from '@/components/DirectoryPickerModal.vue'

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
  })

  if (typeof selected !== 'string') return
  const v = selected.trim()
  if (!v) return

  options.setValue?.(v)

  return v
}
