import type { Ref, ShallowRef } from 'vue'
import { ref, computed } from 'vue'
import type { EditorView } from '@codemirror/view'

const isMobileViewport = () => window.matchMedia('(max-width: 768px)').matches

function cursorInNoFmtZone(view: EditorView): boolean {
  try {
    const head = view.state.selection.main.head
    const dom = view.domAtPos(head)
    let el: Node | null = dom.node.childNodes[dom.offset] ?? dom.node
    while (el && !(el instanceof HTMLElement)) el = el.parentNode
    let node: Element | null = el as Element | null
    while (node && !node.classList.contains('cm-line')) node = node.parentElement
    return !!node?.classList.contains('cm-md-nofmt')
  } catch {
    return false
  }
}

export function useSelectionBar(options: {
  view: ShallowRef<EditorView | undefined>
  editorContainer: Ref<HTMLElement | undefined>
}) {
  const { view, editorContainer } = options

  const selBarOpen = ref(false)
  const selBarLeft = ref(0)
  const selBarTop = ref(0)
  const selBarStyle = computed(() => ({ left: `${selBarLeft.value}px`, top: `${selBarTop.value}px` }))

  function syncSelBar(v: EditorView) {
    if (isMobileViewport() || !v.hasFocus || v.state.selection.main.empty) {
      selBarOpen.value = false
      return
    }
    // Selection sits on a non-formatting region (code block, math block,
    // horizontal rule) — the style toolbar would be noise there.
    if (cursorInNoFmtZone(v)) {
      selBarOpen.value = false
      return
    }
    const range = v.state.selection.main
    const a = v.coordsAtPos(Math.min(range.from, range.to))
    const b = v.coordsAtPos(Math.max(range.from, range.to))
    const rect = editorContainer.value?.getBoundingClientRect()
    if (!a || !b || !rect) {
      selBarOpen.value = false
      return
    }
    const left = (a.left + b.left) / 2 - rect.left
    selBarLeft.value = Math.min(Math.max(left - 130, 8), Math.max(rect.width - 268, 8))
    let y = Math.min(a.top, b.top) - rect.top - 42
    if (y < 4) y = Math.max(b.bottom - rect.top + 6, 4)
    selBarTop.value = y
    selBarOpen.value = true
  }

  function hideSelBar() {
    selBarOpen.value = false
  }

  return { selBarOpen, selBarStyle, syncSelBar, hideSelBar }
}
