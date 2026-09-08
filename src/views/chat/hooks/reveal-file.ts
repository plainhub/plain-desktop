import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import ILucideCopy from '~icons/lucide/copy'
import ILucideDownload from '~icons/lucide/download'
import ILucideFolderOpen from '~icons/lucide/folder-open'
import type { MenuItem } from '@/components/contextmenu/ContextMenuDefine'
import { isLocalMode } from '@/lib/device/local-mode'
import { isMacPlatform } from '@/lib/platform'

export function useRevealFile() {
  const { t } = useI18n()

  function revealFile(path: string) {
    invoke('reveal_chat_file', { uri: path }).catch((err) => console.error('reveal_chat_file failed', err))
  }

  function saveFileAs(path: string, name: string) {
    invoke('save_chat_file_as', { uri: path, name }).catch((err) => console.error('save_chat_file_as failed', err))
  }

  function copyFile(path: string) {
    invoke('copy_chat_file_to_clipboard', { uri: path }).catch((err) => console.error('copy_chat_file_to_clipboard failed', err))
  }

  function buildMediaMenuItems(path: string, name: string = ''): MenuItem[] {
    if (!isLocalMode() || !path || path.startsWith('fsid:')) return []
    const items: MenuItem[] = [
      { label: String(t('show_in_finder')), icon: ILucideFolderOpen, onClick: () => revealFile(path) },
      { label: String(t('save_as')), icon: ILucideDownload, onClick: () => saveFileAs(path, name) },
    ]
    if (isMacPlatform()) {
      items.push({ label: String(t('copy')), icon: ILucideCopy, onClick: () => copyFile(path) })
    }
    return items
  }

  return { revealFile, saveFileAs, copyFile, buildMediaMenuItems }
}
