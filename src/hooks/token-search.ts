import { computed, nextTick, onMounted, onUnmounted, ref, watch, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { keyLabel, keyDescription, displayValue } from '@/lib/token-search-format'
import {
  type Token, type TokenKey,
  isTokenEl, setCaretAtEnd, extractTokensAndText,
  clearAndRender, makeTokenFactory, removeTokensByKey, insertTokenAtCaret,
} from '@/lib/token-search-dom'

type MenuLevel = 'none' | 'key' | 'value'
type ValueOption = string | { value: string; label: string; description?: string }

interface KeyItem { key: TokenKey; label: string; description: string }
interface ValueItem { key: TokenKey; value: string; label: string; description?: string }

export function useTokenSearch(
  props: {
    text: string
    tokens: Token[]
    enterSubmits: boolean
    keyOptions: TokenKey[]
    valueOptions: Record<string, ValueOption[]>
  },
  emit: {
    (e: 'update:text', v: string): void
    (e: 'update:tokens', v: Token[]): void
    (e: 'focus'): void
    (e: 'blur'): void
    (e: 'enter'): void
    (e: 'history:select', v: string): void
    (e: 'history:delete', v: string): void
    (e: 'history:clear'): void
  },
) {
  const { t } = useI18n()

  const rootRef = ref<HTMLElement | null>(null)
  const editableRef = ref<HTMLDivElement | null>(null)
  const valueSearchRef = ref<HTMLInputElement | null>(null)
  const isFocused = ref(false)
  const menuLevel = ref<MenuLevel>('none')
  const selectedKey = ref<TokenKey>('')
  const valueSearch = ref('')
  const activeIndex = ref(0)
  const editingTokenEl = ref<HTMLElement | null>(null)
  const customStartTimeOp = ref('>=')
  const customStartDate = ref('')

  function formatTokenValue(key: TokenKey, value: string) {
    return displayValue(t, props.valueOptions, key, value)
  }

  function updateTokenLabel(token: HTMLElement) {
    const label = token.querySelector('.token-value')
    if (label) label.textContent = formatTokenValue(token.dataset.key ?? '', token.dataset.value ?? '')
  }

  // --- Token DOM factory ---
  const makeToken = makeTokenFactory(
    t,
    formatTokenValue,
    (span) => { span.remove(); syncOut(); nextTick(() => setCaretAtEnd(editableRef.value!)) },
    (span, key) => {
      editingTokenEl.value = span
      selectedKey.value = key
      menuLevel.value = 'value'
      valueSearch.value = ''
      activeIndex.value = 0
      nextTick(() => valueSearchRef.value?.focus())
    },
  )

  // --- Computed ---
  const keyItems = computed<KeyItem[]>(() =>
    (props.keyOptions ?? []).map((k) => ({ key: k, label: keyLabel(t, k), description: keyDescription(t, k) })),
  )

  const selectedKeyLabel = computed(() => keyLabel(t, selectedKey.value))

  const valueItems = computed<ValueItem[]>(() => {
    const k = selectedKey.value
    if (!k) return []
    const raw = (props.valueOptions ?? {})[k] ?? []
    const all = raw
      .map((v) => typeof v === 'string' ? { key: k, value: v, label: v } as ValueItem : { key: k, value: v.value, label: v.label, description: v.description } as ValueItem)
      .filter((it) => it.value)
    const q = valueSearch.value.trim().toLowerCase()
    return q ? all.filter((it) => it.label.toLowerCase().includes(q)) : all
  })

  // --- Menu operations ---
  function openKeyMenu() {
    menuLevel.value = 'key'
    selectedKey.value = ''
    valueSearch.value = ''
    activeIndex.value = 0
    editingTokenEl.value = null
    nextTick(() => editableRef.value?.focus())
  }

  function selectKey(key: TokenKey) {
    selectedKey.value = key
    menuLevel.value = 'value'
    valueSearch.value = ''
    activeIndex.value = 0
    if (key === 'start_time') { customStartTimeOp.value = '>='; customStartDate.value = '' }
    nextTick(() => valueSearchRef.value?.focus())
  }

  function closeMenu() {
    menuLevel.value = 'none'
    selectedKey.value = ''
    valueSearch.value = ''
    activeIndex.value = 0
    editingTokenEl.value = null
  }

  function selectValue(it: ValueItem) {
    if (it.key === 'history') {
      closeMenu()
      editableRef.value?.blur()
      nextTick(() => emit('history:select', it.value))
      return
    }
    const uniqueKeys = new Set(['bucket', 'trash', 'file_size', 'type', 'feed_id', 'duration', 'start_time'])
    if (uniqueKeys.has(it.key) && editableRef.value) removeTokensByKey(editableRef.value, it.key)

    if (editingTokenEl.value && (editingTokenEl.value.dataset.key ?? '') === it.key) {
      editingTokenEl.value.dataset.value = it.value
      updateTokenLabel(editingTokenEl.value)
    } else if (editableRef.value) {
      insertTokenAtCaret(editableRef.value, makeToken(it.key, it.value))
    }
    syncOut()
    editingTokenEl.value = null
    nextTick(() => { editableRef.value?.focus(); openKeyMenu() })
  }

  function applyCustomStartTime() {
    const d = String(customStartDate.value ?? '').trim()
    if (!d) return
    const value = `${customStartTimeOp.value}${d}`
    selectValue({ key: 'start_time', value, label: value })
  }

  function emitHistoryDelete(value: string) { emit('history:delete', value) }
  function emitHistoryClear() { emit('history:clear'); nextTick(() => openKeyMenu()) }

  // --- Sync ---
  function syncOut() {
    if (!editableRef.value) return
    const res = extractTokensAndText(editableRef.value)
    emit('update:tokens', res.tokens)
    emit('update:text', res.text)
  }

  function emitEnter() { closeMenu(); emit('enter') }
  function onInput() { syncOut() }

  function onPaste(e: ClipboardEvent) {
    e.preventDefault()
    const text = e.clipboardData?.getData('text/plain') ?? ''
    const sel = window.getSelection()
    if (!sel || sel.rangeCount === 0) return
    const range = sel.getRangeAt(0)
    range.deleteContents()
    range.insertNode(document.createTextNode(text))
    range.collapse(false)
    sel.removeAllRanges()
    sel.addRange(range)
    syncOut()
  }

  // --- Event handlers ---
  function onKeydown(e: KeyboardEvent) {
    if (e.key === '/' && !e.ctrlKey && !e.metaKey && !e.altKey) { e.preventDefault(); openKeyMenu(); return }
    if (e.key === ' ' && menuLevel.value === 'none') {
      const sel = window.getSelection(); const el = editableRef.value
      if (sel && el && sel.rangeCount > 0) {
        const r = sel.getRangeAt(0)
        if (r.collapsed && el.contains(r.startContainer) && r.startContainer === el && r.startOffset === el.childNodes.length) openKeyMenu()
      }
    }
    if (e.key === 'Escape') { if (menuLevel.value !== 'none') { e.preventDefault(); closeMenu() }; return }
    if (menuLevel.value !== 'none' && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault()
      const max = (menuLevel.value === 'key' ? keyItems.value.length : valueItems.value.length) - 1
      if (max < 0) return
      activeIndex.value = e.key === 'ArrowDown' ? Math.min(activeIndex.value + 1, max) : Math.max(activeIndex.value - 1, 0)
      return
    }
    if (menuLevel.value !== 'none' && (e.key === 'Enter' || e.key === 'Tab')) {
      e.preventDefault()
      if (e.key === 'Enter' && props.enterSubmits) { emitEnter(); return }
      if (menuLevel.value === 'key') { const it = keyItems.value[activeIndex.value]; if (it) selectKey(it.key); return }
      const it = valueItems.value[activeIndex.value]; if (it) selectValue(it)
      return
    }
    if (e.key === 'Enter') { e.preventDefault(); emitEnter(); return }
    if (e.key === 'Backspace') {
      const sel = window.getSelection()
      if (!sel || sel.rangeCount === 0) return
      const r = sel.getRangeAt(0); if (!r.collapsed) return
      const node = r.startContainer
      if (node.nodeType === Node.TEXT_NODE && r.startOffset === 0) {
        const prev = (node as Text).previousSibling
        if (isTokenEl(prev)) { e.preventDefault(); prev.remove(); syncOut() }
      }
    }
  }

  function onFocus() {
    isFocused.value = true; emit('focus')
    if (menuLevel.value === 'none' && keyItems.value.length > 0) openKeyMenu()
  }

  function onBlur() { isFocused.value = false; emit('blur'); nextTick(() => syncOut()) }

  function onMouseDownRoot(e: MouseEvent) {
    if (isFocused.value && menuLevel.value === 'none' && keyItems.value.length > 0) {
      const target = e.target as HTMLElement | null
      if (!target?.closest?.('button.trailing')) openKeyMenu()
    }
    if (editableRef.value && e.target === editableRef.value) nextTick(() => setCaretAtEnd(editableRef.value!))
  }

  function renderFromProps() {
    if (!editableRef.value) return
    clearAndRender(editableRef.value, props.tokens, props.text, makeToken)
  }

  function onDocumentMouseDown(e: MouseEvent) {
    const root = rootRef.value
    if (root && !root.contains(e.target as Node)) closeMenu()
  }

  // --- Watchers & Lifecycle ---
  watch(() => props.valueOptions, () => {
    // Keep the caret and drafted text intact when labels arrive or change.
    for (const node of editableRef.value?.childNodes ?? []) {
      if (isTokenEl(node)) updateTokenLabel(node)
    }
  })

  watch(() => [props.tokens, props.text] as const, () => {
    if (isFocused.value) return
    nextTick(() => renderFromProps())
  }, { deep: true })

  onMounted(() => { renderFromProps(); document.addEventListener('mousedown', onDocumentMouseDown, { capture: true }) })
  onUnmounted(() => { document.removeEventListener('mousedown', onDocumentMouseDown, { capture: true } as any) })

  return {
    rootRef, editableRef, isFocused, menuLevel, selectedKey, selectedKeyLabel,
    valueSearch, activeIndex, customStartTimeOp, customStartDate,
    keyItems, valueItems,
    openKeyMenu, selectKey, selectValue, closeMenu, applyCustomStartTime,
    emitHistoryDelete, emitHistoryClear, emitEnter,
    onKeydown, onInput, onPaste, onFocus, onBlur, onMouseDownRoot,
  }
}
