import { describe, it, expect } from 'vitest'
import {
  splitInGroup,
  removeQuotation,
  detectGroupType,
  splitGroup,
  parseGroup,
  parseQuery,
  buildQuery,
  buildTextFilter,
} from '@/lib/search'
import type { IFilterField } from '@/lib/search'

describe('splitInGroup', () => {
  it('splits a simple space-separated query', () => {
    expect(splitInGroup('foo bar baz')).toEqual(['foo', 'bar', 'baz'])
  })

  it('keeps quoted strings as single tokens', () => {
    expect(splitInGroup('"hello world" foo')).toEqual(['"hello world"', 'foo'])
  })

  it('returns an empty array for empty string', () => {
    expect(splitInGroup('')).toEqual([])
  })

  it('handles single token without spaces', () => {
    expect(splitInGroup('singleton')).toEqual(['singleton'])
  })

  it('keeps a backslash-escaped space inside one token', () => {
    // The phone parses these strings too, and its tokenizer resolves `\`
    // before splitting. A token this one disagreed on is a filter the phone
    // never applies.
    expect(splitInGroup('hello\\ world foo')).toEqual(['hello world', 'foo'])
  })

  it('does not let an escaped quote open a quoted run', () => {
    // Exactly what buildTextFilter emits for `don't stop`.
    expect(splitInGroup("don\\'t\\ stop")).toEqual(["don't stop"])
  })
})

describe('removeQuotation', () => {
  it('removes double quotes', () => {
    expect(removeQuotation('"hello"')).toBe('hello')
  })

  it('removes single quotes', () => {
    expect(removeQuotation("'world'")).toBe('world')
  })

  it('keeps a quote that is not the outer pair', () => {
    // Stripping every quote is what lost the apostrophe in `it's`; the phone
    // only removes a matching pair at both ends.
    expect(removeQuotation(`"it's"`)).toBe(`it's`)
    expect(removeQuotation(`it's`)).toBe(`it's`)
  })

  it('returns string unchanged when no quotes', () => {
    expect(removeQuotation('noquotes')).toBe('noquotes')
  })

  it('handles empty string', () => {
    expect(removeQuotation('')).toBe('')
  })
})

describe('detectGroupType', () => {
  it('detects "=" operator', () => {
    expect(detectGroupType('=active')).toBe('=')
  })

  it('detects ">=" operator', () => {
    expect(detectGroupType('>=100')).toBe('>=')
  })

  it('detects ">" operator', () => {
    expect(detectGroupType('>50')).toBe('>')
  })

  it('detects "!=" operator', () => {
    expect(detectGroupType('!=inactive')).toBe('!=')
  })

  it('detects "<=" operator', () => {
    expect(detectGroupType('<=200')).toBe('<=')
  })

  it('detects "<" operator', () => {
    expect(detectGroupType('<10')).toBe('<')
  })

  it('returns empty string when no operator found', () => {
    expect(detectGroupType('plainvalue')).toBe('')
  })

  it('returns empty string for empty input', () => {
    expect(detectGroupType('')).toBe('')
  })
})

describe('splitGroup', () => {
  it('splits "field:value" into field and value', () => {
    const result = splitGroup('name:Alice')
    expect(result.field).toBe('name')
    expect(result.value).toBe('Alice')
  })

  it('handles operator in value: "field:>=10"', () => {
    const result = splitGroup('size:>=100')
    expect(result.field).toBe('size')
    expect(result.op).toBe('>=')
    expect(result.value).toBe('100')
  })

  it('returns length=1 for bare word (no colon)', () => {
    const result = splitGroup('bareword')
    expect(result.length).toBe(1)
    expect(result.field).toBe('bareword')
  })

  it('preserves colons inside values (joins parts after first colon)', () => {
    const result = splitGroup('url:https://example.com')
    expect(result.field).toBe('url')
    expect(result.value).toBe('https://example.com')
  })
})

describe('parseGroup', () => {
  it('parses a simple field:value group', () => {
    const result = parseGroup('name:Alice')
    expect(result).toEqual({ name: 'name', op: '', value: 'Alice' })
  })

  it('parses "is:X" as { name: X, op: "", value: "true" }', () => {
    const result = parseGroup('is:active')
    expect(result).toEqual({ name: 'active', op: '', value: 'true' })
  })

  it('parses a bare word as a text filter', () => {
    const result = parseGroup('hello')
    expect(result).toEqual({ name: 'text', op: '', value: 'hello' })
  })

  it('parses NOT as a sentinel { name: "", op: "NOT", value: "" }', () => {
    const result = parseGroup('NOT')
    expect(result).toEqual({ name: '', op: 'NOT', value: '' })
  })

  it('parses "field:>=value" with operator', () => {
    const result = parseGroup('size:>=1000')
    expect(result.name).toBe('size')
    expect(result.op).toBe('>=')
    expect(result.value).toBe('1000')
  })

  it('parses "field:!=value" with not-equal operator', () => {
    const result = parseGroup('status:!=deleted')
    expect(result.name).toBe('status')
    expect(result.op).toBe('!=')
    expect(result.value).toBe('deleted')
  })
})

describe('parseQuery', () => {
  it('returns empty array for empty string', () => {
    expect(parseQuery('')).toEqual([])
  })

  it('parses a single bareword as text filter', () => {
    const result = parseQuery('hello')
    expect(result).toEqual([{ name: 'text', op: '', value: 'hello' }])
  })

  it('parses multiple space-separated terms', () => {
    const result = parseQuery('name:Alice age:>18')
    expect(result).toHaveLength(2)
    expect(result[0]).toEqual({ name: 'name', op: '', value: 'Alice' })
    expect(result[1]).toEqual({ name: 'age', op: '>', value: '18' })
  })

  it('inverts the operator of the next term after NOT', () => {
    // NOT name:Alice → name should have inverted op (= becomes !=)
    const result = parseQuery('NOT status:=active')
    const statusField = result.find((f) => f.name === 'status')
    expect(statusField?.op).toBe('!=')
  })

  it('removes the NOT sentinel itself from results', () => {
    const result = parseQuery('NOT status:=active')
    expect(result.every((f) => f.op !== 'NOT')).toBe(true)
  })

  it('handles "is:X" shorthand', () => {
    const result = parseQuery('is:starred')
    expect(result).toEqual([{ name: 'starred', op: '', value: 'true' }])
  })

  it('handles quoted multi-word text search', () => {
    const result = parseQuery('"hello world"')
    expect(result).toEqual([{ name: 'text', op: '', value: 'hello world' }])
  })

  it('handles mixed query with bare words and field filters', () => {
    const result = parseQuery('typescript name:Alice')
    expect(result).toHaveLength(2)
    expect(result[0]).toMatchObject({ name: 'text', value: 'typescript' })
    expect(result[1]).toMatchObject({ name: 'name', value: 'Alice' })
  })

  it('parses field with != operator', () => {
    const result = parseQuery('type:!=video')
    expect(result[0]).toEqual({ name: 'type', op: '!=', value: 'video' })
  })
})

describe('buildQuery', () => {
  it('returns empty string for empty fields array', () => {
    expect(buildQuery([])).toBe('')
  })

  it('builds a simple text query', () => {
    const fields: IFilterField[] = [{ name: 'text', op: '', value: 'hello' }]
    expect(buildQuery(fields)).toBe('hello')
  })

  it('escapes whitespace in a multi-word text value', () => {
    const fields: IFilterField[] = [{ name: 'text', op: '', value: 'hello world' }]
    expect(buildQuery(fields)).toBe('hello\\ world')
  })

  it('drops an empty text value instead of emitting a bare text: token', () => {
    const fields: IFilterField[] = [
      { name: 'text', op: '', value: '  ' },
      { name: 'trash', op: '', value: 'false' },
    ]
    expect(buildQuery(fields)).toBe('trash:false')
  })

  it('builds a field:value query', () => {
    const fields: IFilterField[] = [{ name: 'name', op: '', value: 'Alice' }]
    expect(buildQuery(fields)).toBe('name:Alice')
  })

  it('includes operator in field:op+value', () => {
    const fields: IFilterField[] = [{ name: 'size', op: '>=', value: '100' }]
    expect(buildQuery(fields)).toBe('size:>=100')
  })

  it('wraps multi-word field values in quotes', () => {
    const fields: IFilterField[] = [{ name: 'description', op: '', value: 'multi word value' }]
    expect(buildQuery(fields)).toBe('description:"multi word value"')
  })

  it('skips nullish field values without throwing', () => {
    const fields = [{ name: 'text', op: '', value: undefined } as IFilterField]
    expect(buildQuery(fields)).toBe('')
  })

  it('joins multiple fields with spaces', () => {
    const fields: IFilterField[] = [
      { name: 'text', op: '', value: 'typescript' },
      { name: 'status', op: '!=', value: 'deleted' },
    ]
    expect(buildQuery(fields)).toBe('typescript status:!=deleted')
  })

  it('roundtrips: parseQuery → buildQuery → parseQuery', () => {
    const original = 'name:Alice status:!=deleted typescript'
    const parsed = parseQuery(original)
    const rebuilt = buildQuery(parsed)
    const reparsed = parseQuery(rebuilt)
    expect(reparsed).toEqual(parsed)
  })

  it('roundtrips with a single is: field', () => {
    // is:starred parses to { name: 'starred', op: '', value: 'true' }
    // buildQuery rebuilds as starred:true (not is:starred, different representation)
    const parsed = parseQuery('is:starred')
    const built = buildQuery(parsed)
    expect(built).toBe('starred:true')
  })
})

/**
 * The exact tokens the escaper emits, mirrored by `SearchHelper.buildTextFilter`
 * in plain-app and by `plain-rs`'s own parser tests. All three read the same
 * strings, so a change to any one of them shows up as a red suite rather than
 * as a search that silently stops matching on the phone.
 */
describe('buildTextFilter', () => {
  const cases: [string, string][] = [
    ['', ''],
    ['   ', ''],
    ['hello', 'hello'],
    ['hello world', 'hello\\ world'],
    ['Meeting: notes', 'text:Meeting:\\ notes'],
    ['http://x.com', 'text:http://x.com'],
    ['12:30', 'text:12:30'],
    ["don't", "don\\'t"],
    ['a\\b', 'a\\\\b'],
    ['=foo', '=foo'],
    ['50%_', '50%_'],
  ]

  it.each(cases)('emits the wire token for %j', (input, expected) => {
    expect(buildTextFilter(input)).toBe(expected)
  })

  it.each(cases.filter(([input]) => input.trim() !== ''))(
    'round-trips %j through the parser that reads it',
    (input) => {
      const fields = parseQuery(buildTextFilter(input))
      expect(fields).toHaveLength(1)
      expect(fields[0].name).toBe('text')
      expect(fields[0].value).toBe(input)
    },
  )

  it('keeps the other filters alongside the escaped text', () => {
    const fields = parseQuery(`${buildTextFilter('Meeting: notes')} trash:false ids:1,2`)
    expect(fields.find((f) => f.name === 'text')?.value).toBe('Meeting: notes')
    expect(fields.find((f) => f.name === 'trash')?.value).toBe('false')
    expect(fields.find((f) => f.name === 'ids')?.value).toBe('1,2')
  })
})
