import { ChangeSet, type ChangeSpec } from '@codemirror/state'
import type { EditorView } from '@codemirror/view'

export interface SlashTemplate {
  id: string
  icon: string
  kbd: string
  keywords: string[]
  text: string
  cursor: number
  select?: [number, number]
}

export const SLASH_TEMPLATES: SlashTemplate[] = [
  { id: 'h1', icon: 'H1', kbd: '#', keywords: ['h1', 'heading', 'title'], text: '# ', cursor: 2 },
  { id: 'h2', icon: 'H2', kbd: '##', keywords: ['h2', 'heading', 'title'], text: '## ', cursor: 3 },
  { id: 'h3', icon: 'H3', kbd: '###', keywords: ['h3', 'heading', 'title'], text: '### ', cursor: 4 },
  { id: 'ul', icon: '•', kbd: '-', keywords: ['list', 'bullet', 'ul'], text: '- ', cursor: 2 },
  { id: 'ol', icon: '1.', kbd: '1.', keywords: ['list', 'number', 'ordered', 'ol'], text: '1. ', cursor: 3 },
  { id: 'task', icon: '☑', kbd: '[ ]', keywords: ['task', 'todo', 'check'], text: '- [ ] ', cursor: 6 },
  { id: 'quote', icon: '“', kbd: '>', keywords: ['quote', 'blockquote'], text: '> ', cursor: 2 },
  { id: 'code', icon: '{ }', kbd: '```', keywords: ['code', 'fence'], text: '```\n\n```', cursor: 4 },
  {
    id: 'table',
    icon: '▦',
    kbd: '| |',
    keywords: ['table', 'grid'],
    text: '| Header | Header |\n| ------ | ------ |\n| Cell   | Cell   |',
    cursor: 59,
  },
  { id: 'hr', icon: '—', kbd: '---', keywords: ['divider', 'rule', 'hr'], text: '---', cursor: 3 },
  { id: 'image', icon: 'img', kbd: '!', keywords: ['image', 'picture', 'img'], text: '![alt](url)', cursor: 2, select: [2, 5] },
]

export function matchSlashQuery(textBeforeCursor: string): string | null {
  if (!textBeforeCursor.startsWith('/')) return null
  const query = textBeforeCursor.slice(1)
  return /\s/.test(query) ? null : query
}

export function filterSlashTemplates(query: string): SlashTemplate[] {
  const q = query.toLowerCase()
  if (!q) return SLASH_TEMPLATES
  return SLASH_TEMPLATES.filter(
    (t) => t.id.includes(q) || t.keywords.some((k) => k.includes(q)),
  )
}

export function applyTemplate(view: EditorView, template: SlashTemplate): void {
  const head = view.state.selection.main.head
  view.dispatch({
    changes: { from: head, to: head, insert: template.text },
    selection: template.select
      ? { anchor: head + template.select[0], head: head + template.select[1] }
      : { anchor: head + template.cursor },
  })
}

export function toggleWrap(view: EditorView, marker: string): void {
  const range = view.state.selection.main
  const text = view.state.sliceDoc(range.from, range.to)
  const before = view.state.sliceDoc(Math.max(0, range.from - marker.length), range.from)
  const after = view.state.sliceDoc(range.to, Math.min(view.state.doc.length, range.to + marker.length))
  if (text.startsWith(marker) && text.endsWith(marker) && text.length >= marker.length * 2) {
    view.dispatch({
      changes: { from: range.from, to: range.to, insert: text.slice(marker.length, text.length - marker.length) },
      selection: { anchor: range.from, head: range.to - marker.length * 2 },
    })
  } else if (before === marker && after === marker) {
    view.dispatch({
      changes: [
        { from: range.from - marker.length, to: range.from, insert: '' },
        { from: range.to, to: range.to + marker.length, insert: '' },
      ],
      selection: { anchor: range.from - marker.length, head: range.to - marker.length },
    })
  } else {
    view.dispatch({
      changes: [
        { from: range.from, to: range.from, insert: marker },
        { from: range.to, to: range.to, insert: marker },
      ],
      selection: { anchor: range.from + marker.length, head: range.to + marker.length },
    })
  }
}

const TASK_DONE_PREFIX = '- [x] '
const TASK_PREFIX = '- [ ] '

export function togglePrefix(view: EditorView, prefix: string): void {
  const doc = view.state.doc
  const range = view.state.selection.main
  const startLine = doc.lineAt(range.from).number
  const endLine = doc.lineAt(range.to).number
  const variants = prefix === TASK_PREFIX ? [TASK_PREFIX, TASK_DONE_PREFIX] : [prefix]
  const changes: ChangeSpec[] = []
  let allHave = true
  for (let n = startLine; n <= endLine; n++) {
    const line = doc.line(n)
    if (!variants.some((v) => line.text.startsWith(v))) allHave = false
  }
  for (let n = startLine; n <= endLine; n++) {
    const line = doc.line(n)
    if (allHave) {
      const v = variants.find((v) => line.text.startsWith(v))!
      changes.push({ from: line.from, to: line.from + v.length, insert: '' })
    } else {
      changes.push({ from: line.from, insert: prefix })
    }
  }
  const changeSet = ChangeSet.of(changes, doc.length)
  view.dispatch({
    changes,
    selection: { anchor: changeSet.mapPos(range.anchor, 1), head: changeSet.mapPos(range.head, 1) },
  })
}

const HEADING_STEPS = ['', '# ', '## ', '### ']

export function cycleHeading(view: EditorView): void {
  const doc = view.state.doc
  const range = view.state.selection.main
  const startLine = doc.lineAt(range.from).number
  const endLine = doc.lineAt(range.to).number
  const current = (doc.line(startLine).text.match(/^(#{1,6} )/)?.[1] ?? '')
  const idx = HEADING_STEPS.indexOf(current)
  const next = idx === -1 ? '# ' : HEADING_STEPS[(idx + 1) % HEADING_STEPS.length]
  const changes: ChangeSpec[] = []
  for (let n = startLine; n <= endLine; n++) {
    const line = doc.line(n)
    const had = line.text.match(/^(#{1,6} )/)?.[1] ?? ''
    if (had || next) changes.push({ from: line.from, to: line.from + had.length, insert: next })
  }
  const changeSet = ChangeSet.of(changes, doc.length)
  view.dispatch({
    changes,
    selection: { anchor: changeSet.mapPos(range.anchor, 1), head: changeSet.mapPos(range.head, 1) },
  })
}

export function insertImageAtCursor(view: EditorView): void {
  const range = view.state.selection.main
  const selected = view.state.sliceDoc(range.from, range.to)
  const alt = selected || 'alt'
  const text = `![${alt}](url)`
  view.dispatch({
    changes: { from: range.from, to: range.to, insert: text },
    selection: { anchor: range.from + 2, head: range.from + 2 + alt.length },
  })
}

export function toggleLink(view: EditorView): void {
  const range = view.state.selection.main
  const text = view.state.sliceDoc(range.from, range.to)
  const wrapped = text.match(/^\[([^\]]*)\]\(([^)]*)\)$/)
  if (wrapped) {
    view.dispatch({
      changes: { from: range.from, to: range.to, insert: wrapped[1] },
      selection: { anchor: range.from, head: range.from + wrapped[1].length },
    })
    return
  }
  view.dispatch({
    changes: { from: range.from, to: range.to, insert: `[${text}](url)` },
    selection: { anchor: range.from + text.length + 3, head: range.from + text.length + 6 },
  })
}

export interface GfmTable {
  header: string[]
  align: Array<'left' | 'center' | 'right' | 'none'>
  rows: string[][]
}

const DELIM_ROW = /^\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?$/

export function parseGfmTable(text: string): GfmTable | null {
  const lines = text.split('\n').map((l) => l.trim())
  if (lines.length < 2 || !DELIM_ROW.test(lines[1])) return null
  const splitRow = (line: string) =>
    line
      .replace(/^\|/, '')
      .replace(/\|$/, '')
      .split('|')
      .map((c) => c.trim())
  const header = splitRow(lines[0])
  const align = splitRow(lines[1]).map((c): 'left' | 'center' | 'right' | 'none' => {
    if (c.startsWith(':') && c.endsWith(':')) return 'center'
    if (c.endsWith(':')) return 'right'
    if (c.startsWith(':')) return 'left'
    return 'none'
  })
  if (!align.length || header.length !== align.length) return null
  const rows = lines.slice(2).map(splitRow)
  return { header, align, rows }
}

export interface MathSpan {
  from: number
  to: number
  tex: string
  display: boolean
}

export function findMathSpans(line: string, offset = 0): MathSpan[] {
  const spans: MathSpan[] = []
  let i = 0
  while (i < line.length) {
    if (line[i] === '\\' && (line[i + 1] === '$' || line[i + 1] === '\\')) {
      i += 2
      continue
    }
    if (line[i] !== '$') {
      i++
      continue
    }
    if (line.startsWith('$$', i)) {
      const end = line.indexOf('$$', i + 2)
      if (end > i + 2) {
        spans.push({ from: offset + i, to: offset + end + 2, tex: line.slice(i + 2, end), display: true })
        i = end + 2
        continue
      }
      break
    }
    let j = i + 1
    while (j < line.length && line[j] !== '$') {
      if (line[j] === '\\') j++
      j++
    }
    if (j >= line.length) break
    const content = line.slice(i + 1, j)
    const ok = content.length > 0 && !content.startsWith(' ') && !content.endsWith(' ') && !/^\d/.test(content)
    if (ok) {
      spans.push({ from: offset + i, to: offset + j + 1, tex: content, display: false })
      i = j + 1
    } else {
      i = j + 1
    }
  }
  return spans
}

export function slugifyHeading(text: string): string {
  return text
    .trim()
    .toLowerCase()
    .replace(/[[\]!`*_~()#]/g, '')
    .replace(/[^\p{L}\p{N}\s-]/gu, '')
    .replace(/\s+/g, '-')
    .replace(/^-+|-+$/g, '')
}
