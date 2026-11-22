import { nextTick } from 'vue'
import type { ComposerTranslation } from 'vue-i18n'
import { keyLabel } from '@/lib/token-search-format'

export type TokenKey = string
export interface Token { key: TokenKey; value: string }

export function isTokenEl(node: Node | null): node is HTMLElement {
  return !!node && node.nodeType === Node.ELEMENT_NODE && (node as HTMLElement).dataset?.kind === 'token'
}

export function setCaretAfter(node: Node) {
  const sel = window.getSelection()
  if (!sel) return
  const range = document.createRange()
  range.setStartAfter(node)
  range.collapse(true)
  sel.removeAllRanges()
  sel.addRange(range)
}

export function setCaretAtEnd(el: HTMLElement) {
  const sel = window.getSelection()
  if (!sel) return
  const range = document.createRange()
  range.selectNodeContents(el)
  range.collapse(false)
  sel.removeAllRanges()
  sel.addRange(range)
}

export function extractTokensAndText(el: HTMLElement) {
  const tokens: Token[] = []
  const textParts: string[] = []
  for (const node of Array.from(el.childNodes)) {
    if (isTokenEl(node)) {
      const k = node.dataset.key ?? ''
      const v = node.dataset.value ?? ''
      if (k && v) tokens.push({ key: k, value: v })
      continue
    }
    const t = (node.textContent ?? '').replace(/\u00A0/g, ' ')
    if (t) textParts.push(t)
  }
  return { tokens, text: textParts.join(' ').replace(/\s+/g, ' ').trim() }
}

export function clearAndRender(el: HTMLElement, tokens: Token[], text: string, makeTokenFn: (key: string, value: string) => HTMLElement) {
  el.innerHTML = ''
  for (const t of tokens) {
    el.appendChild(makeTokenFn(t.key, t.value))
    el.appendChild(document.createTextNode(' '))
  }
  if (text) el.appendChild(document.createTextNode(text))
}

export function makeTokenFactory(
  t: ComposerTranslation,
  formatValue: (key: string, value: string) => string,
  onRemove: (span: HTMLElement) => void,
  onEdit: (span: HTMLElement, key: string) => void,
) {
  return function makeToken(key: string, value: string) {
    const span = document.createElement('span')
    span.className = 'token'
    span.dataset.kind = 'token'
    span.dataset.key = key
    span.dataset.value = value
    span.contentEditable = 'false'

    const keySpan = document.createElement('span')
    keySpan.className = 'token-key'
    keySpan.textContent = keyLabel(t, key)

    const sep = document.createElement('span')
    sep.className = 'token-sep'
    sep.textContent = ':'

    const valSpan = document.createElement('span')
    valSpan.className = 'token-value'
    valSpan.textContent = formatValue(key, value)

    const close = document.createElement('button')
    close.className = 'token-remove'
    close.type = 'button'
    close.textContent = '×'
    close.setAttribute('aria-label', 'Remove')
    close.addEventListener('mousedown', (e) => { e.preventDefault(); e.stopPropagation() })
    close.addEventListener('click', (e) => { e.preventDefault(); e.stopPropagation(); onRemove(span) })

    span.addEventListener('mousedown', (e) => {
      if ((e.target as HTMLElement | null)?.classList?.contains('token-remove')) return
      e.preventDefault()
      e.stopPropagation()
      onEdit(span, key)
    })

    span.appendChild(keySpan)
    span.appendChild(sep)
    span.appendChild(valSpan)
    span.appendChild(close)
    return span
  }
}

export function removeTokensByKey(el: HTMLElement, key: string) {
  for (const node of Array.from(el.childNodes)) {
    if (isTokenEl(node) && (node.dataset.key ?? '') === key) node.remove()
  }
}

export function insertTokenAtCaret(el: HTMLElement, tokenEl: HTMLElement) {
  const sel = window.getSelection()
  if (!sel || sel.rangeCount === 0) {
    el.appendChild(tokenEl)
    el.appendChild(document.createTextNode(' '))
    setCaretAfter(tokenEl)
    return
  }
  const range = sel.getRangeAt(0)
  if (!range.collapsed) range.deleteContents()

  if (!el.contains(range.startContainer)) {
    el.appendChild(tokenEl)
    el.appendChild(document.createTextNode(' '))
    setCaretAfter(tokenEl)
    return
  }

  range.insertNode(tokenEl)
  const space = document.createTextNode(' ')
  tokenEl.after(space)
  nextTick(() => setCaretAfter(space))
}
