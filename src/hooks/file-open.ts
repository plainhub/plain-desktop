import { type Ref } from 'vue'
import { canOpenInBrowser, canView, isTextFile, isOfficeFile } from '@/lib/file'

export interface UseFileOpenOptions<T> {
  getName: (item: T) => string
  items: Ref<T[]>
  openTextFile?: (item: T) => void
  openBrowserFile: (item: T) => void
  viewMedia: (items: T[], f: T) => void
  download: (item: T) => void
}

export function useFileOpen<T>(opts: UseFileOpenOptions<T>) {
  const { items, getName, openTextFile, openBrowserFile, viewMedia, download } = opts

  function openFile(item: T) {
    const name = getName(item)
    if ((isTextFile(name) || isOfficeFile(name)) && openTextFile) {
      openTextFile(item)
    } else if (canOpenInBrowser(name)) {
      openBrowserFile(item)
    } else if (canView(name)) {
      viewMedia(items.value, item)
    } else {
      download(item)
    }
  }

  return { openFile }
}
