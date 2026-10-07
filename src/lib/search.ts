

export interface IQueryGroup {
  length: number
  field: string
  query: string
  op: string
  value: string
}

const FILTER_DELIMITER = ':'
const NOT_TYPE = 'NOT'

/**
 * Tokenizer, ported to match `plain-rs`'s `search_dsl::split_in_group` — the
 * phone is what ultimately parses these strings, so a query the two tokenizers
 * disagree on is a filter the phone never applies. The previous regex had no
 * notion of a backslash escape and dropped quotes instead of carrying them,
 * which is why a term containing `'` lost characters here and was still sent on
 * to the phone in a different shape.
 */
export function splitInGroup(s: string) {
  const result: string[] = []
  let buf = ''
  let quote = ''
  let escape = false
  for (const c of s) {
    if (escape) {
      buf += c
      escape = false
    } else if (c === '\\') {
      escape = true
    } else if (quote) {
      buf += c
      if (c === quote) {
        quote = ''
      }
    } else if (c === '"' || c === "'") {
      quote = c
      buf += c
    } else if (/\s/.test(c)) {
      if (buf) {
        result.push(buf)
        buf = ''
      }
    } else {
      buf += c
    }
  }
  if (buf) {
    result.push(buf)
  }
  return result
}
const INVERT: any = {
  '=': '!=',
  '>=': '<',
  '>': '<=',
  '!=': '=',
  '<=': '>',
  '<': '>=',
  in: 'nin',
  nin: 'in',
}
const NUMBER_OPS = ['>', '>=', '<', '<=']
const GROUP_TYPES = Object.keys(INVERT).filter((k: string) => k !== 'in' && k !== 'nin')

export function removeQuotation(s: string) {
  if (s.length >= 2) {
    const first = s[0]
    const last = s[s.length - 1]
    if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
      return s.slice(1, -1)
    }
  }
  return s
}

export function detectGroupType(group: string) {
  return GROUP_TYPES.find((it) => group.indexOf(it) === 0) || ''
}

export function splitGroup(q: string): IQueryGroup {
  const parts = q.split(FILTER_DELIMITER)
  const field = removeQuotation(parts[0])
  const query = removeQuotation(parts.slice(1).join(FILTER_DELIMITER))
  const op = detectGroupType(query)
  const value = op && query.startsWith(op) ? query.slice(op.length) : query

  return {
    length: parts.length,
    field: field,
    query: query,
    op: op,
    value: value,
  }
}

export function parseGroup(group: string): IFilterField {
  if (group == NOT_TYPE) {
    return {
      name: '',
      op: NOT_TYPE,
      value: '',
    }
  }

  const parts = splitGroup(group)
  if (parts.field == 'is') {
    return {
      name: parts.query,
      op: '',
      value: 'true',
    }
  } else if (parts.length == 1) {
    return {
      name: 'text',
      op: '',
      value: parts.field,
    }
  } else {
    return {
      name: parts.field,
      op: parts.op,
      value: parts.value,
    }
  }
}

export interface IFilterField {
  name: string
  op: string
  value: string
}

export const parseQuery = (q: string): IFilterField[] => {
  const groups = splitInGroup(q)?.map((it) => parseGroup(it))
  if (!groups) {
    return []
  }
  let invert = false
  groups.forEach((it) => {
    if (it.op == NOT_TYPE) {
      invert = true
    } else if (invert) {
      it.op = INVERT[it.op] || ''
      invert = false
    }
  })

  return groups.filter((it) => it.op !== NOT_TYPE)
}

/**
 * Turns free text into a filter token the phone's query language can carry.
 *
 * The DSL has no literal-text form: a bare `Meeting: notes` reaches the phone
 * as the *field* `Meeting`, and the notes and media query layers refuse a field
 * the table has no column for — so the search came back as an error instead of
 * results. Escaping the characters the tokenizer treats specially keeps the
 * phrase inside one token. Text without a colon is emitted bare, which the
 * parser reads as a `text` field directly, skipping the operator sniffing that
 * would eat a leading `=`, `<` or `>`.
 *
 * Mirrors `SearchHelper.buildTextFilter` in plain-app; the same table is pinned
 * by tests on both sides and by `plain-rs`'s own parser tests.
 */
export const buildTextFilter = (text: string) => {
  if (!text.trim()) {
    return ''
  }
  const escaped = text.replace(/[\\'"\s]/g, (c) => '\\' + c)
  return text.includes(FILTER_DELIMITER) ? `text${FILTER_DELIMITER}${escaped}` : escaped
}

export const buildQuery = (fileds: IFilterField[]) => {
  const items: string[] = []
  fileds.forEach((it) => {
    if (it.value == null) {
      return
    }

    const value = it.value
    if (it.name === 'text') {
      const token = buildTextFilter(value)
      if (token) {
        items.push(token)
      }
    } else if (value.indexOf(' ') !== -1) {
      items.push(`${it.name}:${it.op}"${value}"`)
    } else {
      items.push(`${it.name}:${it.op}${value}`)
    }
  })

  return items.join(' ')
}
