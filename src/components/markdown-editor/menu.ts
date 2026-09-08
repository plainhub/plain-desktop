import type { Ref, ShallowRef } from 'vue'
import { ref, computed, watch } from 'vue'
import type { EditorView } from '@codemirror/view'
import { i18n } from '@/plugins/i18n'
import {
  SLASH_TEMPLATES,
  matchSlashQuery,
  filterSlashTemplates,
  toggleWrap,
  toggleLink,
  type SlashTemplate,
} from '@/lib/md-editor'

export interface MenuItemRow {
  kind: 'slash' | 'style'
  template?: SlashTemplate
  style?: 'bold' | 'italic' | 'strike' | 'code' | 'link'
  key: string
  icon: string
  label: string
  kbd: string
}

const STYLE_ITEMS: Array<{ style: 'bold' | 'italic' | 'strike' | 'code' | 'link'; icon: string; kbd: string }> = [
  { style: 'bold', icon: '<b>B</b>', kbd: '' },
  { style: 'italic', icon: '<i>I</i>', kbd: '' },
  { style: 'strike', icon: '<s>S</s>', kbd: '' },
  { style: 'code', icon: '&lt;&gt;', kbd: '' },
  { style: 'link', icon: '🔗', kbd: '' },
]

const MENU_WIDTH = 280
const ITEM_HEIGHT = 34

export function useEditorMenu(options: {
  view: ShallowRef<EditorView | undefined>
  editorContainer: Ref<HTMLElement | undefined>
  onOpen?: () => void
}) {
  const { view, editorContainer } = options

  const slashOpen = ref(false)
  const slashQuery = ref('')
  const slashFrom = ref(0)
  const slashActive = ref(0)
  const slashLeft = ref(0)
  const slashTop = ref(0)
  const menuStyleGroup = ref(false)
  const menuSource = ref<'typing' | 'context'>('typing')

  const slashFiltered = computed<SlashTemplate[]>(() => {
    const q = slashQuery.value.toLowerCase()
    if (!q) return SLASH_TEMPLATES
    const merged: SlashTemplate[] = []
    const push = (t: SlashTemplate) => {
      if (!merged.includes(t)) merged.push(t)
    }
    for (const t of filterSlashTemplates(q)) push(t)
    for (const t of SLASH_TEMPLATES) {
      if (i18n.global.t(`md_${t.id}`).toLowerCase().includes(q)) push(t)
    }
    return merged
  })

  watch(slashFiltered, (items) => {
    if (slashActive.value >= items.length) slashActive.value = 0
  })

  const menuItems = computed<MenuItemRow[]>(() => {
    const items: MenuItemRow[] = slashFiltered.value.map((t) => ({
      kind: 'slash' as const,
      template: t,
      key: 'slash-' + t.id,
      icon: t.icon,
      label: i18n.global.t(`md_${t.id}`),
      kbd: t.kbd,
    }))
    if (menuStyleGroup.value && !slashQuery.value) {
      for (const s of STYLE_ITEMS) {
        items.push({
          kind: 'style' as const,
          style: s.style,
          key: 'style-' + s.style,
          icon: s.icon,
          label: i18n.global.t(s.style === 'code' ? 'md_inline_code' : 'md_' + s.style),
          kbd: s.kbd,
        })
      }
    }
    return items
  })

  const styleStart = computed(() => {
    if (!menuStyleGroup.value) return -1
    return slashFiltered.value.length
  })

  const slashStyle = computed(() => ({ left: `${slashLeft.value}px`, top: `${slashTop.value}px` }))

  function closeMenu() {
    slashOpen.value = false
  }

  function syncMenu(v: EditorView) {
    if (!v.hasFocus) {
      closeMenu()
      return
    }
    const head = v.state.selection.main.head
    const line = v.state.doc.lineAt(head)
    const before = v.state.doc.sliceString(line.from, head)
    const query = matchSlashQuery(before)
    if (query === null) {
      closeMenu()
      return
    }
    menuSource.value = 'typing'
    menuStyleGroup.value = false
    slashQuery.value = query
    slashFrom.value = head - 1 - query.length
    if (slashFiltered.value.length === 0) {
      closeMenu()
      return
    }
    const rect = editorContainer.value?.getBoundingClientRect()
    const c = v.coordsAtPos(head)
    if (!rect || !c) {
      closeMenu()
      return
    }
    slashLeft.value = Math.min(Math.max(c.left - rect.left, 8), Math.max(rect.width - MENU_WIDTH, 8))
    const menuH = Math.min(slashFiltered.value.length, 9) * ITEM_HEIGHT + 12
    let y = c.bottom - rect.top + 6
    if (y + menuH > rect.height) y = Math.max(c.top - rect.top - menuH - 6, 4)
    slashTop.value = y
    slashOpen.value = true
  }

  function applySlash(index: number) {
    const v = view.value
    const item = menuItems.value[index]
    if (!v || !item || item.kind !== 'slash' || !item.template) return
    const t = item.template
    const to = v.state.selection.main.head
    const from = slashFrom.value
    v.dispatch({
      changes: { from, to, insert: t.text },
      selection: t.select
        ? { anchor: from + t.select[0], head: from + t.select[1] }
        : { anchor: from + t.cursor },
    })
    v.focus()
    closeMenu()
  }

  function applyMenuItem(index: number) {
    const item = menuItems.value[index]
    if (!item) return
    if (item.kind === 'slash') {
      applySlash(index)
      return
    }
    const v = view.value
    if (!v) return
    if (item.style === 'bold') toggleWrap(v, '**')
    else if (item.style === 'italic') toggleWrap(v, '*')
    else if (item.style === 'strike') toggleWrap(v, '~~')
    else if (item.style === 'code') toggleWrap(v, '`')
    else if (item.style === 'link') toggleLink(v)
    v.focus()
    closeMenu()
  }

  function openMenuAt(x: number, y: number) {
    const v = view.value
    const rect = editorContainer.value?.getBoundingClientRect()
    if (!v || !rect) return
    menuSource.value = 'context'
    menuStyleGroup.value = !v.state.selection.main.empty
    slashQuery.value = ''
    slashFrom.value = v.state.selection.main.head
    slashActive.value = 0
    const menuH = Math.min(menuItems.value.length, 12) * ITEM_HEIGHT + 12
    slashLeft.value = Math.min(Math.max(x, 8), Math.max(rect.width - MENU_WIDTH, 8))
    let top = y
    if (top + menuH > rect.height) top = Math.max(rect.height - menuH - 6, 4)
    slashTop.value = top
    slashOpen.value = true
  }

  function onContextMenu(e: MouseEvent) {
    e.preventDefault()
    options.onOpen?.()
    const rect = editorContainer.value?.getBoundingClientRect()
    if (!rect) return
    openMenuAt(e.clientX - rect.left, e.clientY - rect.top)
  }

  function reanchorContextMenu() {
    const v = view.value
    const rect = editorContainer.value?.getBoundingClientRect()
    if (!v || !rect) return
    const c = v.coordsAtPos(slashFrom.value)
    if (!c) return
    const menuH = Math.min(menuItems.value.length, 12) * ITEM_HEIGHT + 12
    slashLeft.value = Math.min(Math.max(c.left - rect.left, 8), Math.max(rect.width - MENU_WIDTH, 8))
    let top = c.bottom - rect.top + 6
    if (top + menuH > rect.height) top = Math.max(c.top - rect.top - menuH - 6, 4)
    slashTop.value = top
  }

  function onScroll() {
    const v = view.value
    if (!slashOpen.value || !v) return
    if (menuSource.value === 'typing') {
      syncMenu(v)
      return
    }
    reanchorContextMenu()
  }

  const menuKeys = [
    {
      key: 'ArrowDown',
      run: () => {
        if (!slashOpen.value) return false
        slashActive.value = (slashActive.value + 1) % menuItems.value.length
        return true
      },
    },
    {
      key: 'ArrowUp',
      run: () => {
        if (!slashOpen.value) return false
        const n = menuItems.value.length
        slashActive.value = (slashActive.value - 1 + n) % n
        return true
      },
    },
    {
      key: 'Enter',
      run: () => {
        if (!slashOpen.value) return false
        applySlash(slashActive.value)
        return true
      },
    },
    {
      key: 'Escape',
      run: () => {
        if (!slashOpen.value) return false
        closeMenu()
        return true
      },
    },
  ]

  return {
    slashOpen,
    slashActive,
    slashStyle,
    menuItems,
    styleStart,
    applyMenuItem,
    closeMenu,
    syncMenu,
    onContextMenu,
    onScroll,
    menuKeys,
  }
}
