import type { EditorView } from '@codemirror/view'
import { WidgetType } from '@codemirror/view'
import 'katex/dist/katex.min.css'
import katex from 'katex'
import { i18n } from '@/plugins/i18n'
import { copyTextToClipboard } from '@/lib/clipboard'
import type { GfmTable } from '@/lib/md-editor'
import { resolveImageUrl, zoomImage } from './images'

export class CheckboxWidget extends WidgetType {
  constructor(readonly checked: boolean, readonly pos: number, readonly cmView: EditorView) {
    super()
  }
  eq(other: CheckboxWidget) {
    return other.checked === this.checked
  }
  toDOM() {
    const box = document.createElement('span')
    box.className = 'cm-md-task-box' + (this.checked ? ' checked' : '')
    box.setAttribute('role', 'checkbox')
    box.setAttribute('aria-checked', String(this.checked))
    box.textContent = this.checked ? '✓' : ''
    box.addEventListener('mousedown', (e) => {
      e.preventDefault()
      e.stopPropagation()
      this.cmView.dispatch({
        changes: { from: this.pos, to: this.pos + 3, insert: this.checked ? '[ ]' : '[x]' },
        selection: { anchor: this.pos + 3 },
      })
      this.cmView.focus()
    })
    return box
  }
  ignoreEvent() {
    return false
  }
}

export class ImageWidget extends WidgetType {
  constructor(readonly pos: number, readonly link: string, readonly alt: string, readonly cmView: EditorView) {
    super()
  }
  eq(other: ImageWidget) {
    return other.pos === this.pos && other.link === this.link && other.alt === this.alt
  }
  toDOM() {
    const fig = document.createElement('span')
    fig.className = 'cm-md-img'
    fig.dataset.link = this.link
    const img = document.createElement('img')
    img.src = resolveImageUrl(this.link)
    img.alt = this.alt
    img.loading = 'lazy'
    img.addEventListener('error', () => {
      const alt = document.createElement('span')
      alt.className = 'cm-md-img-alt'
      alt.textContent = this.alt || this.link
      fig.replaceChildren(alt)
    })
    fig.appendChild(img)
    fig.addEventListener('mousedown', (e) => {
      e.preventDefault()
      this.cmView.dispatch({ selection: { anchor: this.pos } })
      this.cmView.focus()
    })
    return fig
  }
  ignoreEvent() {
    return false
  }
}

export class CodeHeaderWidget extends WidgetType {
  constructor(readonly lang: string, readonly code: string) {
    super()
  }
  eq(other: CodeHeaderWidget) {
    return other.lang === this.lang && other.code === this.code
  }
  toDOM() {
    const head = document.createElement('span')
    head.className = 'cm-md-codeblock-head'
    head.dataset.code = this.code
    const lang = document.createElement('span')
    lang.className = 'cm-md-codeblock-lang'
    lang.textContent = this.lang || 'text'
    const btn = document.createElement('button')
    btn.type = 'button'
    btn.className = 'cm-md-codeblock-copy'
    btn.textContent = i18n.global.t('copy')
    btn.addEventListener('click', (e) => {
      e.stopPropagation()
      copyTextToClipboard(this.code)
      btn.textContent = '✓'
      setTimeout(() => {
        btn.textContent = i18n.global.t('copy')
      }, 1200)
    })
    head.append(lang, btn)
    return head
  }
  ignoreEvent() {
    return false
  }
}

export class FenceEndWidget extends WidgetType {
  toDOM() {
    return document.createElement('span')
  }
  eq() {
    return true
  }
  ignoreEvent() {
    return true
  }
}

export class HRWidget extends WidgetType {
  constructor(readonly pos: number, readonly cmView: EditorView) {
    super()
  }
  eq(other: HRWidget) {
    return other.pos === this.pos
  }
  toDOM() {
    const hr = document.createElement('span')
    hr.className = 'cm-md-hr'
    hr.addEventListener('mousedown', (e) => {
      e.preventDefault()
      this.cmView.dispatch({ selection: { anchor: this.pos } })
      this.cmView.focus()
    })
    return hr
  }
  ignoreEvent() {
    return false
  }
}

const INLINE_TOKEN = /(\*\*[^*]+\*\*)|(\*[^*]+\*)|(~~[^~]+~~)|(`[^`]+`)|(\[[^\]]*\]\([^)]+\))/g

// Minimal inline markdown renderer for table cells — the whole table is a
// widget, so the editor's line-level mark machinery never runs inside it.
function renderInlineMarkdown(text: string): DocumentFragment {
  const frag = document.createDocumentFragment()
  let last = 0
  for (const m of text.matchAll(INLINE_TOKEN)) {
    if (m.index > last) frag.appendChild(document.createTextNode(text.slice(last, m.index)))
    const token = m[0]
    let el: HTMLElement
    if (token.startsWith('**')) {
      el = document.createElement('strong')
      el.textContent = token.slice(2, -2)
    } else if (token.startsWith('~~')) {
      el = document.createElement('del')
      el.textContent = token.slice(2, -2)
    } else if (token.startsWith('`')) {
      el = document.createElement('code')
      el.textContent = token.slice(1, -1)
    } else if (token.startsWith('[')) {
      const sep = token.indexOf('](')
      el = document.createElement('span')
      el.className = 'cm-md-link'
      el.textContent = token.slice(1, sep)
    } else {
      el = document.createElement('em')
      el.textContent = token.slice(1, -1)
    }
    frag.appendChild(el)
    last = m.index + token.length
  }
  if (last < text.length) frag.appendChild(document.createTextNode(text.slice(last)))
  return frag
}

export class TableWidget extends WidgetType {
  constructor(readonly table: GfmTable, readonly pos: number, readonly cmView: EditorView) {
    super()
  }
  eq(other: TableWidget) {
    return JSON.stringify(other.table) === JSON.stringify(this.table)
  }
  toDOM() {
    const wrap = document.createElement('span')
    wrap.className = 'cm-md-table'
    const table = document.createElement('table')
    const alignOf = (i: number) => this.table.align[i]
    const thead = document.createElement('thead')
    const trh = document.createElement('tr')
    this.table.header.forEach((h, i) => {
      const th = document.createElement('th')
      th.appendChild(renderInlineMarkdown(h))
      const al = alignOf(i)
      if (al !== 'none') th.style.textAlign = al
      trh.appendChild(th)
    })
    thead.appendChild(trh)
    const tbody = document.createElement('tbody')
    this.table.rows.forEach((row) => {
      const tr = document.createElement('tr')
      for (let i = 0; i < this.table.header.length; i++) {
        const td = document.createElement('td')
        td.appendChild(renderInlineMarkdown(row[i] ?? ''))
        const al = alignOf(i)
        if (al !== 'none') td.style.textAlign = al
        tr.appendChild(td)
      }
      tbody.appendChild(tr)
    })
    table.append(thead, tbody)
    wrap.appendChild(table)
    wrap.addEventListener('mousedown', (e) => {
      e.preventDefault()
      this.cmView.dispatch({ selection: { anchor: this.pos } })
      this.cmView.focus()
    })
    return wrap
  }
  ignoreEvent() {
    return false
  }
}

export class MathWidget extends WidgetType {
  constructor(readonly tex: string, readonly display: boolean, readonly pos: number, readonly cmView: EditorView) {
    super()
  }
  eq(other: MathWidget) {
    return other.tex === this.tex && other.display === this.display
  }
  toDOM() {
    const span = document.createElement('span')
    span.className = 'cm-md-math' + (this.display ? ' cm-md-math-display' : '')
    try {
      span.innerHTML = katex.renderToString(this.tex, { throwOnError: false, displayMode: this.display })
    } catch {
      span.textContent = this.tex
    }
    span.addEventListener('mousedown', (e) => {
      e.preventDefault()
      this.cmView.dispatch({ selection: { anchor: this.pos } })
      this.cmView.focus()
    })
    return span
  }
  ignoreEvent() {
    return false
  }
}
