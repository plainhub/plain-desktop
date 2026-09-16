import { describe, expect, it } from 'vitest'
import {
  formatJsonText,
  formatXmlText,
  minifyJsonText,
  parseJsonWithPosition,
  tryFixJson,
  xmlToTreeFromText,
} from '@/views/text-file/json-utils'
import { BIG_FILE_BYTES, cmLanguageFor, fileKindOf, isStructuredKind, kindBadge } from '@/views/text-file/file-kind'

describe('file-kind', () => {
  it('maps extensions to kinds', () => {
    expect(fileKindOf('a.json')).toBe('json')
    expect(fileKindOf('a.XML')).toBe('xml')
    expect(fileKindOf('a.markdown')).toBe('md')
    expect(fileKindOf('a.docx')).toBe('doc')
    expect(fileKindOf('a.log')).toBe('txt')
    expect(fileKindOf('noext')).toBe('txt')
  })

  it('provides badge and language info', () => {
    expect(kindBadge('json')).toBe('JSON')
    expect(kindBadge('doc')).toBe('DOC')
    expect(cmLanguageFor('json', false)).toBe('json')
    expect(cmLanguageFor('json', true)).toBeUndefined()
    expect(isStructuredKind('xml')).toBe(true)
    expect(isStructuredKind('txt')).toBe(false)
    expect(BIG_FILE_BYTES).toBe(8 * 1024 * 1024)
  })
})

describe('parseJsonWithPosition', () => {
  it('returns value for valid json', () => {
    const r = parseJsonWithPosition('{"a":1}')
    expect(r.error).toBeNull()
    expect(r.value).toEqual({ a: 1 })
  })

  it('locates the error line and column', () => {
    const r = parseJsonWithPosition('{\n  "a": 1,\n}')
    expect(r.error).not.toBeNull()
    expect(r.error!.line).toBeGreaterThanOrEqual(2)
    expect(r.error!.column).toBeGreaterThanOrEqual(1)
  })
})

describe('format/minify', () => {
  it('formats and minifies json', () => {
    expect(formatJsonText('{"a":1}')).toBe('{\n  "a": 1\n}')
    expect(minifyJsonText('{\n  "a": 1\n}')).toBe('{"a":1}')
    expect(formatJsonText('nope')).toBeNull()
  })
})

describe('tryFixJson', () => {
  it('removes trailing commas', () => {
    const r = tryFixJson('{"a": 1,}')
    expect(r).not.toBeNull()
    expect(JSON.parse(r!.fixed)).toEqual({ a: 1 })
  })

  it('appends missing closing brackets', () => {
    const r = tryFixJson('{"a": {"b": 1')
    expect(r).not.toBeNull()
    expect(JSON.parse(r!.fixed)).toEqual({ a: { b: 1 } })
  })

  it('returns null for already valid json', () => {
    expect(tryFixJson('{"a": 1}')).toBeNull()
  })
})

describe('xml to tree', () => {
  const xml = `<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.a">
  <uses-permission android:name="p.INTERNET" />
  <uses-permission android:name="p.CAMERA" />
  <application android:label="Plain" />
</manifest>`

  it('builds a tree value with attrs, nested tags and repeated tags as arrays', () => {
    const r = xmlToTreeFromText(xml)
    expect(r).not.toBeNull()
    expect(r!.rootTag).toBe('manifest')
    const value = r!.value
    expect(value['@package']).toBe('com.a')
    const perms = value['uses-permission'] as Record<string, unknown>[]
    expect(perms).toHaveLength(2)
    expect(perms[0]['@android:name']).toBe('p.INTERNET')
  })

  it('returns null for invalid xml', () => {
    expect(xmlToTreeFromText('<manifest><open>')).toBeNull()
  })
})

describe('formatXmlText', () => {
  it('indents nested elements', () => {
    const formatted = formatXmlText('<a><b x="1">t</b><c /></a>')
    expect(formatted).toBe('<a>\n  <b x="1">t</b>\n  <c />\n</a>')
  })

  it('returns null for invalid xml', () => {
    expect(formatXmlText('<a>')).toBeNull()
  })
})
