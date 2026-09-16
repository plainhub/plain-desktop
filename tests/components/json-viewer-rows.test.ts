import { describe, expect, it } from 'vitest'
import {
  buildJsonPath,
  buildRows,
  isNodeCollapsed,
  needsBracketNotation,
} from '@/components/jsonviewer/rows'
import { detectTimestamp } from '@/components/jsonviewer/timestamp'

const state = (expandDepth = 1, expanded: string[] = [], collapsed: string[] = []) => ({
  expanded: new Set(expanded),
  collapsed: new Set(collapsed),
  expandDepth,
})

describe('json path building', () => {
  it('uses dot notation for plain keys', () => {
    expect(buildJsonPath('$', 'name')).toBe('$.name')
    expect(needsBracketNotation('name')).toBe(false)
  })

  it('uses bracket notation for keys with special characters', () => {
    expect(buildJsonPath('$', 'weird.key')).toBe('$["weird.key"]')
    expect(buildJsonPath('$', 'with space')).toBe('$["with space"]')
    expect(needsBracketNotation('a[0]')).toBe(true)
  })
})

describe('buildRows', () => {
  it('flattens objects into open/value/close rows with paths', () => {
    const rows = buildRows({ name: 'plain', meta: { ver: 1 } }, state(100))
    expect(rows.map(r => r.rowKey)).toEqual([
      '$:o', '$.name', '$.meta:o', '$.meta.ver', '$.meta:c', '$:c',
    ])
    expect(rows[1]).toMatchObject({ kind: 'value', path: '$.name', showKey: true, trailingComma: true })
    expect(rows[5]).toMatchObject({ kind: 'close', path: '$', trailingComma: false })
  })

  it('builds array paths with indexes and no key labels', () => {
    const rows = buildRows({ list: ['a', 'b'] }, state(100))
    const valueRows = rows.filter(r => r.kind === 'value')
    expect(valueRows.map(r => r.path)).toEqual(['$.list[0]', '$.list[1]'])
    expect(valueRows.every(r => !r.showKey)).toBe(true)
  })

  it('collapses containers at depth >= expandDepth and reports childCount', () => {
    const rows = buildRows({ a: { b: { c: 1 } } }, state(2))
    const openB = rows.find(r => r.path === '$.a.b' && r.kind === 'open')
    expect(openB).toMatchObject({ isCollapsed: true, childCount: 1, depth: 2 })
    expect(rows.filter(r => r.kind === 'value')).toHaveLength(0)
    const openAgain = buildRows({ a: { b: { c: 1 } } }, state(2, ['$.a.b']))
    expect(openAgain.find(r => r.path === '$.a.b')).toMatchObject({ isCollapsed: false })
    expect(openAgain.filter(r => r.kind === 'value')).toHaveLength(1)
  })

  it('lets collapsed overrides win over expanded default depth', () => {
    const rows = buildRows({ a: { b: 1 } }, state(100, [], ['$.a']))
    expect(rows.find(r => r.path === '$.a')).toMatchObject({ isCollapsed: true, trailingComma: false })
    expect(rows.map(r => r.rowKey)).toEqual(['$:o', '$.a:o', '$:c'])
  })

  it('keeps trailing comma on a collapsed open row only', () => {
    const rows = buildRows({ first: { x: 1 }, last: { y: 2 } }, state(1, [], []))
    const openFirst = rows.find(r => r.path === '$.first')
    const openLast = rows.find(r => r.path === '$.last')
    expect(openFirst).toMatchObject({ isCollapsed: true, trailingComma: true })
    expect(openLast).toMatchObject({ isCollapsed: true, trailingComma: false })
  })

  it('flattens a primitive root into a single value row', () => {
    const rows = buildRows('hello', state(1))
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({ kind: 'value', path: '$', showKey: false })
  })

  it('isNodeCollapsed honors expanded then collapsed then depth', () => {
    const s = state(2, ['$.x'], ['$.y'])
    expect(isNodeCollapsed('$.x', 9, s)).toBe(false)
    expect(isNodeCollapsed('$.y', 0, s)).toBe(true)
    expect(isNodeCollapsed('$.z', 1, s)).toBe(false)
    expect(isNodeCollapsed('$.z', 2, s)).toBe(true)
  })
})

describe('detectTimestamp', () => {
  it('detects unix seconds within 2000-2100', () => {
    const info = detectTimestamp(1600000000)
    expect(info).toMatchObject({ type: 'unix-s', original: 1600000000 })
    expect(info!.formatted).not.toBe('')
  })

  it('detects unix milliseconds', () => {
    expect(detectTimestamp(1600000000000)).toMatchObject({ type: 'unix-ms' })
  })

  it('detects ISO date-time strings', () => {
    expect(detectTimestamp('2026-09-16T12:30:00Z')).toMatchObject({ type: 'iso' })
  })

  it('rejects out-of-range numbers and plain strings', () => {
    expect(detectTimestamp(42)).toBeNull()
    expect(detectTimestamp(946684799)).toBeNull()
    expect(detectTimestamp(4102444801)).toBeNull()
    expect(detectTimestamp('hello world')).toBeNull()
    expect(detectTimestamp(null)).toBeNull()
  })
})
