import { describe, it, expect } from 'vitest'
import { EditorView } from '@codemirror/view'
import { EditorState } from '@codemirror/state'
import {
  SLASH_TEMPLATES,
  matchSlashQuery,
  filterSlashTemplates,
  applyTemplate,
  toggleWrap,
  togglePrefix,
  cycleHeading,
  insertImageAtCursor,
  toggleLink,
  parseGfmTable,
  slugifyHeading,
  findMathSpans,
} from '@/lib/md-editor'

function makeView(doc: string, from = 0, to = 0) {
  const view = new EditorView({
    state: EditorState.create({ doc, extensions: [] }),
    parent: document.createElement('div'),
  })
  view.dispatch({ selection: { anchor: from, head: to } })
  return view
}


describe('matchSlashQuery', () => {
  it('matches a bare slash at line start', () => {
    expect(matchSlashQuery('/')).toBe('')
  })

  it('returns the typed query after the slash', () => {
    expect(matchSlashQuery('/hea')).toBe('hea')
  })

  it('rejects when the slash is not at line start', () => {
    expect(matchSlashQuery('hello /')).toBeNull()
    expect(matchSlashQuery('x/hea')).toBeNull()
  })

  it('rejects once whitespace is typed', () => {
    expect(matchSlashQuery('/hea ')).toBeNull()
    expect(matchSlashQuery('/head ing')).toBeNull()
  })

  it('rejects empty text', () => {
    expect(matchSlashQuery('')).toBeNull()
  })
})

describe('filterSlashTemplates', () => {
  it('returns all templates for an empty query', () => {
    expect(filterSlashTemplates('')).toEqual(SLASH_TEMPLATES)
  })

  it('filters by keyword', () => {
    const ids = filterSlashTemplates('heading').map((t) => t.id)
    expect(ids).toEqual(['h1', 'h2', 'h3'])
  })

  it('filters by id', () => {
    expect(filterSlashTemplates('h2').map((t) => t.id)).toEqual(['h2'])
  })

  it('returns nothing for an unmatched query', () => {
    expect(filterSlashTemplates('zzz')).toEqual([])
  })
})

describe('SLASH_TEMPLATES', () => {
  it('has unique ids', () => {
    const ids = SLASH_TEMPLATES.map((t) => t.id)
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('keeps the cursor inside the inserted text', () => {
    for (const t of SLASH_TEMPLATES) {
      expect(t.cursor).toBeGreaterThanOrEqual(0)
      expect(t.cursor).toBeLessThanOrEqual(t.text.length)
    }
  })

  it('builds a valid table skeleton', () => {
    const table = SLASH_TEMPLATES.find((t) => t.id === 'table')!
    const lines = table.text.split('\n')
    expect(lines).toHaveLength(3)
    expect(lines[1]).toMatch(/^\|(?:[\s-]*\|)+$/)
  })
})

describe('applyTemplate', () => {
  it('inserts the template text at the cursor', () => {
    const view = makeView('hello')
    const table = SLASH_TEMPLATES.find((t) => t.id === 'table')!
    applyTemplate(view, table)
    expect(view.state.doc.toString()).toBe('| Header | Header |\n| ------ | ------ |\n| Cell   | Cell   |hello')
  })
})

describe('toggleWrap', () => {
  it('wraps the selection with the marker', () => {
    const view = makeView('hello world', 0, 5)
    toggleWrap(view, '**')
    expect(view.state.doc.toString()).toBe('**hello** world')
  })

  it('unwraps when the selection already carries the marker', () => {
    const view = makeView('**hello** world', 0, 9)
    toggleWrap(view, '**')
    expect(view.state.doc.toString()).toBe('hello world')
  })

  it('unwraps markers around (not inside) the selection', () => {
    const view = makeView('**hello**', 2, 7)
    toggleWrap(view, '**')
    expect(view.state.doc.toString()).toBe('hello')
  })
})

describe('togglePrefix', () => {
  it('adds the prefix to every selected line', () => {
    const view = makeView('a\nb', 0, 3)
    togglePrefix(view, '- ')
    expect(view.state.doc.toString()).toBe('- a\n- b')
  })

  it('removes the prefix when every selected line has it', () => {
    const view = makeView('- a\n- b', 0, 7)
    togglePrefix(view, '- ')
    expect(view.state.doc.toString()).toBe('a\nb')
  })

  it('treats done tasks as having the task prefix', () => {
    const view = makeView('- [x] done', 6, 10)
    togglePrefix(view, '- [ ] ')
    expect(view.state.doc.toString()).toBe('done')
  })

  it('adds the task prefix to a plain line', () => {
    const view = makeView('todo item', 0, 0)
    togglePrefix(view, '- [ ] ')
    expect(view.state.doc.toString()).toBe('- [ ] todo item')
  })
})

describe('cycleHeading', () => {
  it('cycles plain → h1 → h2 → h3 → plain', () => {
    const view = makeView('Title')
    cycleHeading(view)
    expect(view.state.doc.toString()).toBe('# Title')
    cycleHeading(view)
    expect(view.state.doc.toString()).toBe('## Title')
    cycleHeading(view)
    expect(view.state.doc.toString()).toBe('### Title')
    cycleHeading(view)
    expect(view.state.doc.toString()).toBe('Title')
  })

  it('normalizes a deeper heading into the cycle', () => {
    const view = makeView('#### Deep')
    cycleHeading(view)
    expect(view.state.doc.toString()).toBe('# Deep')
  })
})

describe('insertImageAtCursor', () => {
  it('inserts an image template and selects the alt placeholder', () => {
    const view = makeView('x', 1, 1)
    insertImageAtCursor(view)
    expect(view.state.doc.toString()).toBe('x![alt](url)')
    expect(view.state.selection.main.head).toBe(6)
  })
})

describe('toggleLink', () => {
  it('wraps the selection into a markdown link and selects the url', () => {
    const view = makeView('click here', 0, 10)
    toggleLink(view)
    expect(view.state.doc.toString()).toBe('[click here](url)')
    expect(view.state.selection.main.head).toBe(16)
  })

  it('unwraps an existing link back to its text', () => {
    const view = makeView('[click here](url)', 0, 17)
    toggleLink(view)
    expect(view.state.doc.toString()).toBe('click here')
  })
})

describe('parseGfmTable', () => {
  it('parses header, alignment and rows', () => {
    const t = parseGfmTable('| 指标 | 前 | 后 |\n| :--- | ---: | :---: |\n| 冷启动 | 240ms | 90ms |')!
    expect(t.header).toEqual(['指标', '前', '后'])
    expect(t.align).toEqual(['left', 'right', 'center'])
    expect(t.rows).toEqual([['冷启动', '240ms', '90ms']])
  })

  it('rejects a block without a delimiter row', () => {
    expect(parseGfmTable('| a | b |\n| x | y |')).toBeNull()
    expect(parseGfmTable('just text')).toBeNull()
  })

  it('tolerates missing outer pipes', () => {
    const t = parseGfmTable('a | b\n--- | ---\n1 | 2')!
    expect(t.header).toEqual(['a', 'b'])
    expect(t.rows).toEqual([['1', '2']])
  })
})

describe('findMathSpans', () => {
  it('finds inline math', () => {
    const spans = findMathSpans('energy $E = mc^2$ done')
    expect(spans).toHaveLength(1)
    expect(spans[0]).toMatchObject({ tex: 'E = mc^2', display: false })
  })

  it('finds single-line display math', () => {
    const spans = findMathSpans('$$x + y$$')
    expect(spans).toHaveLength(1)
    expect(spans[0]).toMatchObject({ tex: 'x + y', display: true })
  })

  it('ignores currency amounts', () => {
    expect(findMathSpans('it costs $5 to $10')).toEqual([])
  })

  it('ignores escaped dollars', () => {
    expect(findMathSpans('price \\$5')).toEqual([])
  })

  it('reports offsets relative to the provided base', () => {
    const spans = findMathSpans('ab $x$', 100)
    expect(spans[0].from).toBe(103)
    expect(spans[0].to).toBe(106)
  })
})

describe('slugifyHeading', () => {
  it('slugifies a plain heading', () => {
    expect(slugifyHeading('Table of Contents')).toBe('table-of-contents')
  })

  it('strips punctuation and collapses whitespace like the blog TOC links', () => {
    expect(slugifyHeading('Method 1: PlainApp — no app on the iPhone')).toBe(
      'method-1-plainapp-no-app-on-the-iphone',
    )
  })

  it('strips markdown markers before slugging', () => {
    expect(slugifyHeading('# **Bold** Title')).toBe('bold-title')
  })
})
