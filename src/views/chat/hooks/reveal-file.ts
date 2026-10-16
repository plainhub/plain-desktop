import { revealLocalFile, saveLocalFileAs, copyLocalFile } from '@/lib/native-file-actions'
import { useI18n } from 'vue-i18n'
import ILucideCopy from '~icons/lucide/copy'
import ILucideDownload from '~icons/lucide/download'
import ILucideFolderOpen from '~icons/lucide/folder-open'
import type { MenuItem } from '@/components/contextmenu/ContextMenuDefine'
import { isLocalMode } from '@/lib/device/local-mode'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { hasFileExport, hasFileReveal, hasFileClipboard } from '@/lib/feature'

export function useRevealFile() {
  const { t } = useI18n()
  const { app } = storeToRefs(useTempStore())

  function revealFile(path: string) {
    revealLocalFile(path).catch((err) => console.error('reveal file failed', err))
  }

  function saveFileAs(path: string, name: string) {
    saveLocalFileAs(path, name).catch((err) => console.error('save file failed', err))
  }

  function copyFile(path: string) {
    copyLocalFile(path).catch((err) => console.error('copy file failed', err))
  }

  function buildMediaMenuItems(path: string, name: string = ''): MenuItem[] {
    if (!isLocalMode() || !path || path.startsWith('fsid:')) return []
    const items: MenuItem[] = []
    if (hasFileReveal(app.value)) {
      items.push({ label: String(t('show_in_finder')), icon: ILucideFolderOpen, onClick: () => revealFile(path) })
    }
    if (hasFileExport(app.value)) {
      items.push({ label: String(t('save_as')), icon: ILucideDownload, onClick: () => saveFileAs(path, name) })
    }
    if (hasFileClipboard(app.value)) {
      items.push({ label: String(t('copy')), icon: ILucideCopy, onClick: () => copyFile(path) })
    }
    return items
  }

  return { revealFile, saveFileAs, copyFile, buildMediaMenuItems }
}
