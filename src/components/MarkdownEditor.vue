<template>
  <div ref="editorContainer" class="markdown-editor md-container" @paste="handlePaste" @drop.prevent="handleDrop" @dragover.prevent @contextmenu="onContextMenu">
    <div v-if="slashOpen" class="slash-menu" :style="slashStyle" @mousedown.prevent>
      <template v-for="(it, i) in menuItems" :key="it.key">
        <div v-if="it.kind === 'style' && i === styleStart" class="slash-div"></div>
        <div
          class="slash-item"
          :class="{ hot: i === slashActive }"
          @click="applyMenuItem(i)"
          @mousemove="slashActive = i"
        >
          <span class="slash-ic" v-html="it.icon"></span>
          <span class="slash-label">{{ it.label }}</span>
          <span class="slash-kbd">{{ it.kbd }}</span>
        </div>
      </template>
    </div>
    <div v-if="selBarOpen" class="sel-toolbar" :style="selBarStyle" @mousedown.prevent>
      <button class="st-btn" :aria-label="$t('md_bold')" @click="runCmd(toggleWrap, '**')"><b>B</b></button>
      <button class="st-btn" :aria-label="$t('md_italic')" @click="runCmd(toggleWrap, '*')"><i>I</i></button>
      <button class="st-btn" :aria-label="$t('md_strike')" @click="runCmd(toggleWrap, '~~')"><s>S</s></button>
      <button class="st-btn" :aria-label="$t('md_inline_code')" @click="runCmd(toggleWrap, '`')">&lt;&gt;</button>
      <button class="st-btn" :aria-label="$t('md_link')" @click="runCmd(toggleLink)">🔗</button>
      <span class="st-div"></span>
      <button class="st-btn" :aria-label="$t('md_h1')" @click="runCmd(cycleHeading)">H</button>
      <button class="st-btn" :aria-label="$t('md_ul')" @click="runCmd(togglePrefix, '- ')">•</button>
      <button class="st-btn" :aria-label="$t('md_task')" @click="runCmd(togglePrefix, '- [ ] ')">☑</button>
      <button class="st-btn" :aria-label="$t('md_quote')" @click="runCmd(togglePrefix, '> ')">❝</button>
    </div>
    <div class="fmt-bar">
      <button class="fb" :aria-label="$t('md_h1')" @mousedown.prevent @click="runCmd(cycleHeading)">H</button>
      <button class="fb" :aria-label="$t('md_bold')" @mousedown.prevent @click="runCmd(toggleWrap, '**')"><b>B</b></button>
      <button class="fb" :aria-label="$t('md_italic')" @mousedown.prevent @click="runCmd(toggleWrap, '*')"><i>I</i></button>
      <button class="fb" :aria-label="$t('md_strike')" @mousedown.prevent @click="runCmd(toggleWrap, '~~')"><s>S</s></button>
      <button class="fb" :aria-label="$t('md_task')" @mousedown.prevent @click="runCmd(togglePrefix, '- [ ] ')">☑</button>
      <button class="fb" :aria-label="$t('md_ul')" @mousedown.prevent @click="runCmd(togglePrefix, '- ')">•</button>
      <button class="fb" :aria-label="$t('md_inline_code')" @mousedown.prevent @click="runCmd(toggleWrap, '`')">&lt;&gt;</button>
      <button class="fb" :aria-label="$t('md_quote')" @mousedown.prevent @click="runCmd(togglePrefix, '> ')">❝</button>
      <button class="fb" :aria-label="$t('md_table')" @mousedown.prevent @click="insertTable">▦</button>
      <button class="fb" :aria-label="$t('md_image')" @mousedown.prevent @click="runCmd(insertImageAtCursor)">img</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, shallowRef, onMounted, onUnmounted, watch } from 'vue'
import { EditorView, keymap, placeholder as cmPlaceholder } from '@codemirror/view'
import { EditorState, type Extension } from '@codemirror/state'
import type { SyntaxNode } from '@lezer/common'
import { markdown, markdownLanguage } from '@codemirror/lang-markdown'
import { languages } from '@codemirror/language-data'
import { defaultKeymap, indentWithTab, history, historyKeymap } from '@codemirror/commands'
import { syntaxTree } from '@codemirror/language'
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete'
import { searchKeymap, highlightSelectionMatches } from '@codemirror/search'
import emitter from '@/plugins/eventbus'
import {
  SLASH_TEMPLATES,
  slugifyHeading,
  applyTemplate,
  toggleWrap,
  togglePrefix,
  cycleHeading,
  toggleLink,
  insertImageAtCursor,
} from '@/lib/md-editor'
import { mdCodeHighlight } from '@/lib/md-code-highlight'
import { livePreviewPlugin } from './markdown-editor/decorations'
import { baseTheme, lightTheme, darkThemeOverride, mdHighlightExtensions } from './markdown-editor/theme'
import { useEditorMenu } from './markdown-editor/menu'
import { useSelectionBar } from './markdown-editor/selection'

const props = defineProps<{
  modelValue: string
  placeholder?: string
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string]
  'paste-images': [files: File[]]
}>()

const editorContainer = ref<HTMLElement>()
const view = shallowRef<EditorView>()

const { selBarOpen, selBarStyle, syncSelBar, hideSelBar } = useSelectionBar({ view, editorContainer })
const menu = useEditorMenu({ view, editorContainer, onOpen: hideSelBar })
const { slashOpen, slashActive, slashStyle, menuItems, styleStart, applyMenuItem, closeMenu, syncMenu, onContextMenu, onScroll, menuKeys } = menu

function runCmd(fn: (v: EditorView, ...args: never[]) => void, ...args: unknown[]) {
  const v = view.value
  if (!v) return
  ;(fn as (v: EditorView, ...a: unknown[]) => void)(v, ...args)
  v.focus()
}

function insertTable() {
  const v = view.value
  const table = SLASH_TEMPLATES.find((t) => t.id === 'table')
  if (!v || !table) return
  applyTemplate(v, table)
  v.focus()
}

let isDark = document.documentElement.classList.contains('dark')

function getExtensions(): Extension[] {
  const exts: Extension[] = [
    history(),
    closeBrackets(),
    highlightSelectionMatches(),
    markdown({ base: markdownLanguage, codeLanguages: languages }),
    livePreviewPlugin,
    mdHighlightExtensions,
    mdCodeHighlight,
    keymap.of([...menuKeys, ...defaultKeymap, ...historyKeymap, ...closeBracketsKeymap, ...searchKeymap, indentWithTab]),
    EditorView.lineWrapping,
    EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        emit('update:modelValue', update.state.doc.toString())
      }
      if (update.docChanged || update.selectionSet || update.focusChanged) {
        syncMenu(update.view)
        syncSelBar(update.view)
      }
    }),
  ]
  if (props.placeholder) exts.push(cmPlaceholder(props.placeholder))
  exts.push(baseTheme, lightTheme, isDark ? darkThemeOverride : lightTheme)
  return exts
}

function createEditor() {
  if (!editorContainer.value) return
  view.value = new EditorView({
    state: EditorState.create({ doc: props.modelValue, extensions: getExtensions() }),
    parent: editorContainer.value,
  })
}

function handlePaste(e: ClipboardEvent) {
  const items = e.clipboardData?.items
  if (!items) return
  const images: File[] = []
  for (const item of items) {
    if (item.kind === 'file' && item.type.startsWith('image/')) {
      const file = item.getAsFile()
      if (file) images.push(file)
    }
  }
  if (images.length > 0) {
    e.preventDefault()
    emit('paste-images', images)
  }
}

function handleDrop(e: DragEvent) {
  const files = e.dataTransfer?.files
  if (!files) return
  const images: File[] = []
  for (const file of files) {
    if (file.type.startsWith('image/')) images.push(file)
  }
  if (images.length > 0) emit('paste-images', images)
}

function insertText(text: string) {
  const v = view.value
  if (!v) return
  const { from } = v.state.selection.main
  v.dispatch({ changes: { from, insert: text }, selection: { anchor: from + text.length } })
  v.focus()
}

function replaceTheme() {
  const v = view.value
  if (!v) return
  const doc = v.state.doc.toString()
  closeMenu()
  v.destroy()
  view.value = new EditorView({
    state: EditorState.create({ doc, extensions: getExtensions() }),
    parent: editorContainer.value!,
  })
}

function colorModeChangedHandler() {
  const dark = document.documentElement.classList.contains('dark')
  if (dark === isDark) return
  isDark = dark
  replaceTheme()
}

// Theme can change through paths other than the event bus (system theme,
// external class toggles). Watch the root class directly so the editor's
// syntax theme never falls out of sync with the CSS theme.
const rootClassObserver = new MutationObserver(colorModeChangedHandler)

function onEditorScroll() {
  hideSelBar()
  menu.onScroll()
}

function slugOfHeadingLine(view: EditorView, lineNumber: number): string | null {
  const doc = view.state.doc
  const line = doc.line(lineNumber)
  if (/^\s*\#{1,6}\s/.test(line.text)) return slugifyHeading(line.text.replace(/^\s*\#{1,6}\s/, ''))
  const next = lineNumber < doc.lines ? doc.line(lineNumber + 1) : null
  if (next && /^\s*(=+|-+)\s*$/.test(next.text) && line.text.trim()) return slugifyHeading(line.text)
  return null
}

function onEditorClick(e: MouseEvent) {
  const el = (e.target as HTMLElement)?.closest('.cm-md-link') as HTMLElement | null
  const v = view.value
  if (!el || !v) return
  const pos = v.posAtDOM(el)
  let link: SyntaxNode | null = syntaxTree(v.state).resolveInner(pos, -1)
  while (link && link.name !== 'Link') link = link.parent
  if (!link) return
  let href = ''
  for (let child = link.firstChild; child; child = child.nextSibling) {
    if (child.name === 'URL') href = v.state.sliceDoc(child.from, child.to)
  }
  if (!href) return
  e.preventDefault()
  if (href.startsWith('#')) {
    const slug = slugifyHeading(decodeURIComponent(href.slice(1)))
    const doc = v.state.doc
    for (let n = 1; n <= doc.lines; n++) {
      if (slugOfHeadingLine(v, n) === slug) {
        const line = doc.line(n)
        v.dispatch({ selection: { anchor: line.from }, effects: EditorView.scrollIntoView(line.from, { y: 'start' }) })
        return
      }
    }
    return
  }
  if (/^https?:\/\//.test(href)) window.open(href, '_blank', 'noopener')
}

onMounted(() => {
  createEditor()
  editorContainer.value?.addEventListener('scroll', onEditorScroll, true)
  editorContainer.value?.addEventListener('click', onEditorClick)
  emitter.on('color_mode_changed', colorModeChangedHandler)
  rootClassObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })
})

onUnmounted(() => {
  editorContainer.value?.removeEventListener('scroll', onEditorScroll, true)
  editorContainer.value?.removeEventListener('click', onEditorClick)
  rootClassObserver.disconnect()
  view.value?.destroy()
  emitter.off('color_mode_changed', colorModeChangedHandler)
})

// Sync external value changes (e.g., loading from server)
watch(
  () => props.modelValue,
  (val) => {
    const v = view.value
    if (!v || v.state.doc.toString() === val) return
    v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: val } })
  },
)

defineExpose({ insertText })
</script>

<style scoped>
.markdown-editor {
  height: 100%;
  overflow: hidden;
  position: relative;
}
.markdown-editor :deep(.cm-editor) {
  height: 100%;
}
.markdown-editor :deep(.cm-md-strong) {
  font-weight: 650;
}
.markdown-editor :deep(.cm-md-em) {
  font-style: italic;
}
.markdown-editor :deep(.cm-md-strike) {
  text-decoration: line-through;
}
.markdown-editor :deep(.cm-md-mark) {
  color: var(--md-sys-color-on-surface-variant);
  font-family: 'SF Mono', 'Fira Code', 'Fira Mono', Menlo, Consolas, monospace;
  font-size: 0.85em;
}

.slash-menu {
  position: absolute;
  z-index: 20;
  width: 264px;
  max-height: 320px;
  overflow-y: auto;
  border-radius: 12px;
  border: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface-container-lowest);
  box-shadow: 0 8px 24px rgba(26, 27, 38, 0.16);
  padding: 6px;
  font-size: 13.5px;
}
.slash-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  border-radius: 8px;
  color: var(--md-sys-color-on-surface);
  cursor: pointer;
}
.slash-item.hot {
  background: var(--md-sys-color-surface-container);
}
.slash-ic {
  width: 26px;
  height: 26px;
  border-radius: 6px;
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--md-sys-color-surface-container);
  color: var(--md-sys-color-on-surface-variant);
  font-size: 12px;
  font-weight: 600;
  font-family: 'SF Mono', 'Fira Code', Menlo, Consolas, monospace;
}
.slash-item.hot .slash-ic {
  background: var(--md-sys-color-primary-container);
  color: var(--md-sys-color-on-primary-container);
}
.slash-label {
  flex: 1;
}
.slash-kbd {
  font-family: 'SF Mono', 'Fira Code', Menlo, Consolas, monospace;
  font-size: 10.5px;
  color: var(--md-sys-color-on-surface-variant);
}
.slash-div {
  height: 1px;
  background: var(--md-sys-color-outline-variant);
  margin: 4px 6px;
}

.sel-toolbar {
  position: absolute;
  z-index: 25;
  display: inline-flex;
  align-items: center;
  gap: 1px;
  background: var(--md-sys-color-surface-container-high);
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 10px;
  padding: 4px;
  box-shadow: 0 6px 20px rgba(26, 27, 38, 0.2);
}
.st-btn {
  min-width: 28px;
  height: 28px;
  border-radius: 7px;
  border: none;
  cursor: pointer;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-family: ui-monospace, Menlo, Consolas, monospace;
  font-size: 13px;
  padding: 0 5px;
}
.st-btn:hover {
  background: var(--md-sys-color-surface-container);
  color: var(--md-sys-color-on-surface);
}
.st-div {
  width: 1px;
  height: 16px;
  background: var(--md-sys-color-outline-variant);
  margin: 0 3px;
}

.markdown-editor :deep(img.cm-widgetBuffer) {
  margin: 0;
  max-width: none;
  border-radius: 0;
}
.markdown-editor :deep(.cm-md-task-box) {
  display: inline-flex;
  width: 17px;
  height: 17px;
  border-radius: 4.5px;
  border: 1.5px solid var(--md-sys-color-outline);
  margin-right: 6px;
  vertical-align: -3px;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  color: transparent;
  user-select: none;
}
.markdown-editor :deep(.cm-md-task-box.checked) {
  background: var(--md-sys-color-primary);
  border-color: var(--md-sys-color-primary);
  color: #fff;
}
.markdown-editor :deep(.cm-task-done) {
  text-decoration: line-through;
  color: var(--md-sys-color-on-surface-variant);
}

.markdown-editor :deep(.cm-md-img) {
  line-height: 0;
}
.markdown-editor :deep(.cm-md-img img) {
  display: inline-block;
  vertical-align: top;
  max-height: 320px;
  max-width: 100%;
}

.markdown-editor :deep(.cm-md-codeblock-head-line) {
  display: flex;
  align-items: center;
}
.markdown-editor :deep(.cm-md-codeblock-head-line .cm-md-codeblock-head) {
  flex: 1;
}
.markdown-editor :deep(.cm-md-codeblock-head) {
  display: inline-flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  user-select: none;
}
.markdown-editor :deep(.cm-md-codeblock-copy) {
  border: none;
  cursor: pointer;
  border-radius: 6px;
  padding: 2px 8px;
  font-size: 12px;
  background: transparent;
}
.markdown-editor :deep(.cm-md-codeblock-end-line) {
  line-height: 0;
  font-size: 0;
  padding: 0;
}

.markdown-editor :deep(.cm-md-collapse-line) {
  line-height: 1px;
  font-size: 1px;
  padding: 0;
}
.markdown-editor :deep(.cm-md-table-line) {
  display: flex;
}
.markdown-editor :deep(.cm-md-table-line .cm-md-table) {
  flex: 1;
}
.markdown-editor :deep(.cm-md-table) {
  display: block;
  overflow-x: auto;
}
.markdown-editor :deep(.cm-md-table table) {
  border-collapse: collapse;
  font-size: 0.92em;
}
.markdown-editor :deep(.cm-md-table tr:hover td) {
  background: var(--md-sys-color-surface-container);
}
.markdown-editor :deep(.cm-md-hr-line) {
  display: flex;
}
.markdown-editor :deep(.cm-md-hr) {
  flex: 1;
}

.markdown-editor :deep(.cm-md-math) {
  white-space: normal;
}
.markdown-editor :deep(.cm-md-math-display) {
  display: block;
  overflow-x: auto;
  padding: 6px 0;
}
.markdown-editor :deep(.cm-md-math-display .katex-display) {
  margin: 0;
}
.markdown-editor :deep(.cm-md-math-block-line) {
  display: flex;
  padding: 2px 0;
}
.markdown-editor :deep(.cm-md-math-block-line .cm-md-math) {
  flex: 1;
}

.fmt-bar {
  display: none;
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 15;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px calc(6px + env(safe-area-inset-bottom));
  background: var(--md-sys-color-surface-container-low);
  border-top: 1px solid var(--md-sys-color-outline-variant);
}
.fb {
  width: 33px;
  height: 33px;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 14px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-family: ui-monospace, Menlo, Consolas, monospace;
  cursor: pointer;
}
.fb:active {
  background: var(--md-sys-color-surface-container-high);
  color: var(--md-sys-color-on-surface);
}

@media (max-width: 768px) {
  .fmt-bar {
    display: flex;
  }
  .sel-toolbar {
    display: none;
  }
  .markdown-editor :deep(.cm-content) {
    padding-bottom: 96px;
  }
}
</style>
