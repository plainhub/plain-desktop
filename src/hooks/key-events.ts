import { isRef, type ComputedRef, type Ref } from 'vue'

function isInputFocused() {
  const activeElement = document.activeElement
  if (!activeElement) return false
  return activeElement.tagName === 'INPUT' || activeElement.tagName === 'TEXTAREA' || activeElement.tagName === 'SELECT' || (activeElement as HTMLElement).isContentEditable
}

export const useKeyEvents = (total: Ref<number>, limit: Ref<number> | ComputedRef<number> | number, page: Ref<number>, selectAll: () => void, clearSelection: () => void, gotoPage: (page: number) => void, deleteItems: () => void) => {
  return {
    keyDown: (e: KeyboardEvent) => {
      if (document.querySelector('md-dialog[open]') || document.getElementsByClassName('lightbox').length > 0) {
        return
      }
      const content = document.getElementsByClassName('scroll-content')?.[0] as HTMLDivElement
      if (e.shiftKey) {
        content?.style.setProperty('user-select', 'none')
      } else {
        content?.style.removeProperty('user-select')
      }

      const content2 = document.getElementsByClassName('scroller')?.[0] as HTMLDivElement
      if (e.shiftKey) {
        content2?.style.setProperty('user-select', 'none')
      } else {
        content2?.style.removeProperty('user-select')
      }

      if ((e.ctrlKey || e.metaKey) && e.key === 'a' && !isInputFocused()) {
        e.preventDefault()
        selectAll()
        return
      }

      // esc key to clear selection
      if (e.key === 'Escape') {
        clearSelection()
      } else if (e.key === 'ArrowLeft') {
        if (page.value > 1) {
          gotoPage(page.value - 1)
        }
      } else if (e.key === 'ArrowRight') {
        const limitVal = isRef(limit) ? limit.value : limit
        const pages = Math.ceil(total.value / limitVal)
        if (page.value < pages) {
          gotoPage(page.value + 1)
        }
      } else if (e.key === 'Delete' || ((e.ctrlKey || e.metaKey) && e.key === 'Backspace')) {
        deleteItems()
      }
    },
    keyUp: () => {
      const content = document.getElementsByClassName('scroll-content')?.[0] as HTMLDivElement
      content?.style.removeProperty('user-select')
    },
  }
}

export const useFilesKeyEvents = (selectAll: () => void, clearSelection: () => void, deleteItems: () => void) => {
  return {
    keyDown: (e: KeyboardEvent) => {
      if (document.querySelector('md-dialog[open]')) {
        return
      }
      const content = document.getElementsByClassName('scroll-content')?.[0] as HTMLDivElement
      if (e.shiftKey) {
        content?.style.setProperty('user-select', 'none')
      } else {
        content?.style.removeProperty('user-select')
      }

      const content2 = document.getElementsByClassName('scroller')?.[0] as HTMLDivElement
      if (e.shiftKey) {
        content2?.style.setProperty('user-select', 'none')
      } else {
        content2?.style.removeProperty('user-select')
      }

      if ((e.ctrlKey || e.metaKey) && e.key === 'a' && !isInputFocused()) {
        e.preventDefault()
        selectAll()
        return
      }

      // esc key to clear selection
      if (e.key === 'Escape') {
        clearSelection()
      } else if (e.key === 'Delete' || ((e.ctrlKey || e.metaKey) && e.key === 'Backspace')) {
        if (e.repeat || document.querySelector('.v-modal-backdrop, .lightbox')) return
        if (isInputFocused() && !document.activeElement?.matches('input[type="checkbox"]')) return
        e.preventDefault()
        deleteItems()
      }
    },
    keyUp: () => {
      const content = document.getElementsByClassName('scroll-content')?.[0] as HTMLDivElement
      content?.style.removeProperty('user-select')
    },
  }
}
