import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from '@codemirror/view'
import { RangeSet, type Range } from '@codemirror/state'
import type { SyntaxNodeRef } from '@lezer/common'
import { syntaxTree } from '@codemirror/language'
import { findMathSpans, parseGfmTable } from '@/lib/md-editor'
import {
  CheckboxWidget,
  CodeHeaderWidget,
  FenceEndWidget,
  HRWidget,
  ImageWidget,
  MathWidget,
  TableWidget,
} from './widgets'

function isCursorInside(view: EditorView, from: number, to: number): boolean {
  return view.state.selection.ranges.some((r) => r.from <= to && r.to >= from)
}

export function buildLiveDecorations(v: EditorView): DecorationSet {
  const ranges: Range<Decoration>[] = []
  const hidden: Array<{ from: number; to: number }> = []
  const doc = v.state.doc
  const codeRanges: Array<{ from: number; to: number }> = []

  const addReplace = (from: number, to: number, widget?: WidgetType) => {
    if (to <= from) return
    if (hidden.some((h) => from < h.to && h.from < to)) return
    hidden.push({ from, to })
    ranges.push(Decoration.replace(widget ? { widget } : {}).range(from, to))
  }
  const addMark = (from: number, to: number, cls: string) => {
    if (to <= from) return
    ranges.push(Decoration.mark({ class: cls }).range(from, to))
  }
  const addLine = (from: number, cls: string) => {
    ranges.push(Decoration.line({ class: cls }).range(from))
  }
  const overlapsCode = (from: number, to: number) =>
    codeRanges.some((c) => from < c.to && c.from < to)

  const markNames = new Set(['EmphasisMark', 'CodeMark', 'StrikethroughMark'])

  const handleInline = (node: SyntaxNodeRef, cls: string) => {
    const active = isCursorInside(v, node.from, node.to)
    addMark(node.from, node.to, cls)
    const marks: Array<{ from: number; to: number }> = []
    for (let child = node.node.firstChild; child; child = child.nextSibling) {
      if (markNames.has(child.name)) marks.push({ from: child.from, to: child.to })
    }
    if (marks.length === 0) return
    const first = marks[0]
    const last = marks[marks.length - 1]
    if (active) {
      addMark(first.from, first.to, 'cm-md-mark')
      if (last !== first) addMark(last.from, last.to, 'cm-md-mark')
    } else {
      addReplace(first.from, first.to)
      if (last !== first) addReplace(last.from, last.to)
    }
  }

  const handleHeading = (node: SyntaxNodeRef, level: number) => {
    const active = isCursorInside(v, node.from, node.to)
    addLine(node.from, `cm-md-hline cm-md-h${level}-line`)
    addMark(node.from, node.to, `cm-md-h${level}`)
    const mark = node.node.firstChild
    if (mark && mark.name === 'HeaderMark') {
      const end = doc.sliceString(mark.to, mark.to + 1) === ' ' ? mark.to + 1 : mark.to
      if (active) addMark(mark.from, end, 'cm-md-mark')
      else addReplace(mark.from, end)
    }
  }

  const handleFenced = (node: SyntaxNodeRef) => {
    codeRanges.push({ from: node.from, to: node.to })
    const firstLine = doc.lineAt(node.from)
    const lastLine = doc.lineAt(node.to)
    const openMatch = firstLine.text.match(/^\s*(`{3,}|~{3,})(.*)$/)
    const lang = (openMatch?.[2] ?? '').trim().split(/\s+/)[0]
    const closeIsFence = lastLine.number > firstLine.number && /^\s*(`{3,}|~{3,})\s*$/.test(lastLine.text)
    if (isCursorInside(v, node.from, node.to)) {
      addLine(firstLine.from, 'cm-md-codeblock-line cm-md-codeblock-head-line')
      for (let n = firstLine.number + 1; n <= lastLine.number; n++) {
        const line = doc.line(n)
        addLine(line.from, 'cm-md-codeblock-line' + (n === lastLine.number ? ' cm-md-codeblock-last' : ''))
      }
      return
    }
    const codeEnd = closeIsFence ? Math.max(firstLine.to + 1, lastLine.from - 1) : node.to
    const code = doc.sliceString(Math.min(firstLine.to + 1, codeEnd), codeEnd)
    ranges.push(Decoration.replace({ widget: new CodeHeaderWidget(lang, code) }).range(firstLine.from, firstLine.to))
    addLine(firstLine.from, 'cm-md-codeblock-head-line')
    const lastCodeNumber = closeIsFence ? lastLine.number - 1 : lastLine.number
    for (let n = firstLine.number + 1; n <= lastLine.number; n++) {
      if (closeIsFence && n === lastLine.number) continue
      const line = doc.line(n)
      const isFirstCode = n === firstLine.number + 1
      const isLastCode = n === lastCodeNumber
      const base = 'cm-md-codeblock-line' + (isFirstCode ? ' cm-md-codeblock-first-code' : '') + (isLastCode ? ' cm-md-codeblock-last' : '')
      addLine(line.from, base)
    }
    if (closeIsFence) {
      ranges.push(Decoration.replace({ widget: new FenceEndWidget() }).range(lastLine.from, lastLine.to))
      addLine(lastLine.from, 'cm-md-codeblock-end-line')
    }
  }

  const handleTask = (node: SyntaxNodeRef) => {
    let marker: SyntaxNodeRef | null = null
    for (let child = node.node.firstChild; child; child = child.nextSibling) {
      if (child.name === 'TaskMarker') {
        marker = child
        break
      }
    }
    if (!marker) return
    const checked = doc.sliceString(marker.from, marker.to) === '[x]'
    ranges.push(Decoration.replace({ widget: new CheckboxWidget(checked, marker.from, v) }).range(marker.from, marker.to))
    if (checked && node.to > marker.to) addMark(marker.to, node.to, 'cm-task-done')
  }

  const handleImage = (node: SyntaxNodeRef) => {
    if (isCursorInside(v, node.from, node.to)) return
    const m = doc.sliceString(node.from, node.to).match(/^!\[([^\]]*)\]\(([^)]+)\)$/)
    if (!m) return
    ranges.push(Decoration.replace({ widget: new ImageWidget(node.from, m[2], m[1], v) }).range(node.from, node.to))
  }

  const handleTable = (node: SyntaxNodeRef) => {
    if (isCursorInside(v, node.from, node.to)) return
    const table = parseGfmTable(doc.sliceString(node.from, node.to))
    if (!table) return
    const firstLine = doc.lineAt(node.from)
    const lastLine = doc.lineAt(node.to)
    ranges.push(Decoration.replace({ widget: new TableWidget(table, node.from, v) }).range(firstLine.from, firstLine.to))
    addLine(firstLine.from, 'cm-md-table-line')
    for (let n = firstLine.number + 1; n <= lastLine.number; n++) {
      const line = doc.line(n)
      addReplace(line.from, line.to, new FenceEndWidget())
      addLine(line.from, 'cm-md-collapse-line')
    }
  }

  const handleHR = (node: SyntaxNodeRef) => {
    if (isCursorInside(v, node.from, node.to)) return
    const line = doc.lineAt(node.from)
    addReplace(line.from, line.to, new HRWidget(node.from, v))
    addLine(line.from, 'cm-md-hr-line cm-md-nofmt')
  }

  const handleLink = (node: SyntaxNodeRef) => {
    addMark(node.from, node.to, 'cm-md-link')
    if (isCursorInside(v, node.from, node.to)) {
      for (let child = node.node.firstChild; child; child = child.nextSibling) {
        if (child.name === 'LinkMark') addMark(child.from, child.to, 'cm-md-mark')
      }
      return
    }
    for (let child = node.node.firstChild; child; child = child.nextSibling) {
      if (child.name === 'LinkMark') addReplace(child.from, child.to)
    }
  }

  const handleSetext = (node: SyntaxNodeRef, level: number) => {
    const active = isCursorInside(v, node.from, node.to)
    const firstLine = doc.lineAt(node.from)
    const lastLine = doc.lineAt(node.to)
    addLine(firstLine.from, `cm-md-hline cm-md-h${level}-line`)
    addMark(firstLine.from, firstLine.to, `cm-md-h${level}`)
    if (active) {
      addMark(lastLine.from, lastLine.to, 'cm-md-mark')
    } else {
      addReplace(lastLine.from, lastLine.to)
      addLine(lastLine.from, 'cm-md-collapse-line')
    }
  }

  const handleIndentedCode = (node: SyntaxNodeRef) => {
    codeRanges.push({ from: node.from, to: node.to })
    const firstLine = doc.lineAt(node.from)
    const lastLine = doc.lineAt(node.to)
    for (let n = firstLine.number; n <= lastLine.number; n++) {
      const line = doc.line(n)
      const first = n === firstLine.number ? ' cm-md-codeblock-first-code' : ''
      const last = n === lastLine.number ? ' cm-md-codeblock-last' : ''
      addLine(line.from, `cm-md-codeblock-line${first}${last} cm-md-nofmt`)
    }
  }

  const handleBlockquote = (node: SyntaxNodeRef) => {
    const firstLine = doc.lineAt(node.from)
    const lastLine = doc.lineAt(node.to)
    for (let n = firstLine.number; n <= lastLine.number; n++) {
      const line = doc.line(n)
      const modifier = n === firstLine.number ? ' cm-md-quote-first' : n === lastLine.number ? ' cm-md-quote-last' : ''
      addLine(line.from, `cm-md-quote-line${modifier}`)
    }
    for (let child = node.node.firstChild; child; child = child.nextSibling) {
      if (child.name === 'QuoteMark') addReplace(child.from, child.to)
    }
  }

  const handleListMark = (node: SyntaxNodeRef) => {
    const item = node.node.parent
    let depth = 0
    let isOrdered = false
    let isTask = false
    for (let cur = item; cur; cur = cur.parent) {
      if (cur.name === 'BulletList' || cur.name === 'OrderedList') {
        depth++
        if (cur.name === 'OrderedList') isOrdered = true
      }
    }
    for (let child = item?.firstChild; child; child = child.nextSibling) {
      if (child.name === 'Task') isTask = true
    }
    const d = Math.min(Math.max(depth, 1), 4)
    const kind = isTask ? 'task' : isOrdered ? 'ol' : 'ul'
    addLine(node.from, `cm-md-list-line cm-md-${kind}-d${d}`)
    const markText = doc.sliceString(node.from, node.to)
    if (!isOrdered && (markText === '-' || markText === '*' || markText === '+')) addReplace(node.from, node.to)
  }

  const scanMath = () => {
    const consumed = new Set<number>()
    for (const { from, to } of v.visibleRanges) {
      const first = doc.lineAt(from).number
      const last = doc.lineAt(to).number
      for (let n = first; n <= last; n++) {
        if (consumed.has(n)) continue
        const line = doc.line(n)
        if (overlapsCode(line.from, line.to)) continue
        const trimmed = line.text.trim()
        const inlineSpans = findMathSpans(line.text, line.from)
        const sameLineDisplay = inlineSpans.some((s) => s.display)
        if (trimmed.startsWith('$$') && !sameLineDisplay) {
          let closeLine: typeof line | null = null
          for (let m = n + 1; m <= Math.min(n + 500, doc.lines); m++) {
            const candidate = doc.line(m)
            if (candidate.text.trim().endsWith('$$')) {
              closeLine = candidate
              break
            }
          }
          if (closeLine) {
            for (let m = n; m <= closeLine.number; m++) consumed.add(m)
            if (isCursorInside(v, line.from, closeLine.to)) continue
            const texParts = [line.text.trim().slice(2)]
            for (let m = n + 1; m < closeLine.number; m++) texParts.push(doc.line(m).text.trim())
            texParts.push(closeLine.text.trim().slice(0, -2))
            const tex = texParts.filter((p) => p.length > 0).join('\n')
            addReplace(line.from, line.to, new MathWidget(tex, true, line.from, v))
            addLine(line.from, 'cm-md-math-block-line cm-md-nofmt')
            for (let m = n + 1; m <= closeLine.number; m++) {
              const mid = doc.line(m)
              addReplace(mid.from, mid.to, new FenceEndWidget())
              addLine(mid.from, 'cm-md-collapse-line cm-md-nofmt')
            }
            continue
          }
        }
        for (const span of inlineSpans) {
          if (consumed.has(n)) break
          if (codeRanges.some((c) => span.from < c.to && c.from < span.to)) continue
          addReplace(span.from, span.to, new MathWidget(span.tex, span.display, span.from, v))
        }
      }
    }
  }

  for (const { from, to } of v.visibleRanges) {
    syntaxTree(v.state).iterate({
      from, to,
      enter(node) {
        switch (node.name) {
          case 'ATXHeading1': handleHeading(node, 1); break
          case 'ATXHeading2': handleHeading(node, 2); break
          case 'ATXHeading3': handleHeading(node, 3); break
          case 'ATXHeading4': handleHeading(node, 4); break
          case 'ATXHeading5': handleHeading(node, 5); break
          case 'ATXHeading6': handleHeading(node, 6); break
          case 'StrongEmphasis': handleInline(node, 'cm-md-strong'); break
          case 'Emphasis': handleInline(node, 'cm-md-em'); break
          case 'Strikethrough': handleInline(node, 'cm-md-strike'); break
          case 'InlineCode':
            codeRanges.push({ from: node.from, to: node.to })
            handleInline(node, 'cm-md-code-inline')
            break
          case 'FencedCode': handleFenced(node); break
          case 'CodeBlock': handleIndentedCode(node); break
          case 'Task': handleTask(node); break
          case 'Image': handleImage(node); break
          case 'Table': handleTable(node); break
          case 'HorizontalRule': handleHR(node); break
          case 'Blockquote': handleBlockquote(node); break
          case 'ListMark': handleListMark(node); break
          case 'Link': handleLink(node); break
          case 'SetextHeading1': handleSetext(node, 1); break
          case 'SetextHeading2': handleSetext(node, 2); break
        }
      },
    })
  }
  scanMath()
  return RangeSet.of(ranges, true)
}

export const livePreviewPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet
    constructor(v: EditorView) {
      this.decorations = buildLiveDecorations(v)
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged || update.selectionSet) {
        this.decorations = buildLiveDecorations(update.view)
      }
    }
  },
  { decorations: (v) => v.decorations },
)
