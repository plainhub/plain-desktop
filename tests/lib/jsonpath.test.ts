import { describe, expect, it } from 'vitest'
import {
  buildJsonPath,
  collectJsonPathMatches,
  evaluateJsonPath,
  generateSuggestions,
  needsBracketNotation,
} from '@/lib/jsonpath'

const data = {
  name: 'plain',
  apps: [
    { packageName: 'com.a', installedAt: 1600000000, tags: ['x', 'y'] },
    { packageName: 'com.b', installedAt: 1630000000, tags: [] },
  ],
  'weird.key': 1,
  flags: { adb: true },
}

describe('jsonpath path building', () => {
  it('uses dot or bracket notation', () => {
    expect(buildJsonPath('$', 'name')).toBe('$.name')
    expect(buildJsonPath('$', 'weird.key')).toBe('$["weird.key"]')
    expect(needsBracketNotation('with space')).toBe(true)
  })
})

describe('evaluateJsonPath', () => {
  it('resolves nested member paths', () => {
    expect(evaluateJsonPath(data, '$.name')).toEqual(['plain'])
    expect(evaluateJsonPath(data, '$.flags.adb')).toEqual([true])
    expect(evaluateJsonPath(data, '$["weird.key"]')).toEqual([1])
  })

  it('supports array index, wildcard and recursive descent', () => {
    expect(evaluateJsonPath(data, '$.apps[0].packageName')).toEqual(['com.a'])
    expect(evaluateJsonPath(data, '$.apps[*].packageName')).toEqual(['com.a', 'com.b'])
    expect(evaluateJsonPath(data, '$..installedAt')).toEqual([1600000000, 1630000000])
  })

  it('supports filter expressions', () => {
    expect(evaluateJsonPath(data, '$.apps[?(@.packageName=="com.b")].installedAt')).toEqual([1630000000])
  })
})

describe('collectJsonPathMatches', () => {
  it('returns paths matching the tree row path format', () => {
    expect(collectJsonPathMatches(data, '$.name')).toEqual(['$.name'])
    expect(collectJsonPathMatches(data, '$.apps[1].packageName')).toEqual(['$.apps[1].packageName'])
    expect(collectJsonPathMatches(data, '$..installedAt')).toEqual([
      '$.apps[0].installedAt',
      '$.apps[1].installedAt',
    ])
  })

  it('handles bracket notation keys and root', () => {
    expect(collectJsonPathMatches(data, '$["weird.key"]')).toEqual(['$["weird.key"]'])
    expect(collectJsonPathMatches(data, '$')).toEqual(['$'])
  })
})

describe('generateSuggestions', () => {
  it('suggests child keys for the parent path', () => {
    const suggestions = generateSuggestions(data, '$.flags.')
    expect(suggestions.some(s => s.path === '$.flags.adb')).toBe(true)
  })

  it('suggests array indexes and wildcards', () => {
    const suggestions = generateSuggestions(data, '$.apps[')
    expect(suggestions.some(s => s.path === '$.apps[0]')).toBe(true)
    expect(suggestions.some(s => s.path === '$.apps[*]')).toBe(true)
  })
})
