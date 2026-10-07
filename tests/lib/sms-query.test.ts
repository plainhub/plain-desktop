import { describe, it, expect } from 'vitest'
import { parseQuery, buildQuery } from '@/lib/search'
import type { IFilterField } from '@/lib/search'

describe('SMS query building', () => {
  it('builds thread_id query', () => {
    const fields: IFilterField[] = [{ name: 'thread_id', op: '', value: '17' }]
    const query = buildQuery(fields)
    expect(query).toBe('thread_id:17')
  })

  it('builds thread_id with archived query', () => {
    const fields: IFilterField[] = [
      { name: 'thread_id', op: '', value: '17' },
      { name: 'archived', op: '', value: '1' },
    ]
    const query = buildQuery(fields)
    expect(query).toBe('thread_id:17 archived:1')
  })

  it('builds empty query for empty fields', () => {
    const query = buildQuery([])
    expect(query).toBe('')
  })

  it('builds type filter query', () => {
    const fields: IFilterField[] = [{ name: 'type', op: '', value: '1' }]
    const query = buildQuery(fields)
    expect(query).toBe('type:1')
  })

  it('builds text search query', () => {
    const fields: IFilterField[] = [{ name: 'text', op: '', value: 'hello' }]
    const query = buildQuery(fields)
    expect(query).toBe('hello')
  })

  it('builds text search with spaces as one escaped token', () => {
    // Escaped rather than quoted: the phone's tokenizer resolves `\` before
    // splitting, and its removeQuotation only strips a matching pair at both
    // ends — so a quoted value containing an apostrophe lost the character.
    const fields: IFilterField[] = [{ name: 'text', op: '', value: 'hello world' }]
    const query = buildQuery(fields)
    expect(query).toBe('hello\\ world')
    expect(parseQuery(query)).toEqual([{ name: 'text', op: '', value: 'hello world' }])
  })
})

describe('SMS query parsing', () => {
  it('parses thread_id:17', () => {
    const result = parseQuery('thread_id:17')
    expect(result).toEqual([{ name: 'thread_id', op: '', value: '17' }])
  })

  it('parses thread_id:17 archived:1', () => {
    const result = parseQuery('thread_id:17 archived:1')
    expect(result).toHaveLength(2)
    expect(result).toContainEqual({ name: 'thread_id', op: '', value: '17' })
    expect(result).toContainEqual({ name: 'archived', op: '', value: '1' })
  })

  it('parses type:1', () => {
    const result = parseQuery('type:1')
    expect(result).toEqual([{ name: 'type', op: '', value: '1' }])
  })

  it('roundtrips thread_id + archived query', () => {
    const original: IFilterField[] = [
      { name: 'thread_id', op: '', value: '17' },
      { name: 'archived', op: '', value: '1' },
    ]
    const query = buildQuery(original)
    const parsed = parseQuery(query)
    expect(parsed).toHaveLength(2)
    expect(parsed).toContainEqual({ name: 'thread_id', op: '', value: '17' })
    expect(parsed).toContainEqual({ name: 'archived', op: '', value: '1' })
  })
})
