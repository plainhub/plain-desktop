export function isEditableTarget(target: EventTarget | null) {
  const el = target as HTMLElement | null
  if (!el) return false
  const tag = el.tagName?.toLowerCase()
  return tag === 'input' || tag === 'textarea' || (el as any).isContentEditable
}

export function selectedTextWithin(el: Element | null) {
  if (!el) return ''
  const sel = window.getSelection()
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return ''
  if (!el.contains(sel.getRangeAt(0).commonAncestorContainer)) return ''
  return sel.toString()
}
