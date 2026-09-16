export interface JsonParseResult {
  value: unknown
  error: { message: string; line: number; column: number } | null
}

export function parseJsonWithPosition(text: string): JsonParseResult {
  try {
    return { value: JSON.parse(text), error: null }
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e)
    let line = 1
    let column = 1
    const posMatch = message.match(/position (\d+)/)
    if (posMatch) {
      const pos = parseInt(posMatch[1])
      const before = text.slice(0, pos)
      line = before.split('\n').length
      column = pos - before.lastIndexOf('\n')
    }
    return { value: undefined, error: { message, line, column } }
  }
}

export function formatJsonText(text: string, indent: number | string = 2): string | null {
  try {
    return JSON.stringify(JSON.parse(text), null, indent)
  } catch {
    return null
  }
}

export function minifyJsonText(text: string): string | null {
  try {
    return JSON.stringify(JSON.parse(text))
  } catch {
    return null
  }
}

export function tryFixJson(input: string): { fixed: string; changes: string[] } | null {
  const changes: string[] = []
  let text = input

  if (text.charCodeAt(0) === 0xFEFF) {
    text = text.slice(1)
    changes.push('bom')
  }

  const singleLineCommentRegex = /\/\/[^\n]*/g
  if (singleLineCommentRegex.test(text)) {
    text = text.replace(new RegExp(singleLineCommentRegex.source, 'g'), '')
    changes.push('comments')
  }

  const multiLineCommentRegex = /\/\*[\s\S]*?\*\//g
  if (multiLineCommentRegex.test(text)) {
    text = text.replace(new RegExp(multiLineCommentRegex.source, 'g'), '')
    changes.push('comments')
  }

  if (/'\s*:/.test(text) || /:\s*'/.test(text)) {
    text = replaceSingleQuotes(text)
    changes.push('quotes')
  }

  const unquotedKeyRegex = /(?<=[{,]\s*)([a-zA-Z_$][a-zA-Z0-9_$]*)\s*:/g
  if (unquotedKeyRegex.test(text)) {
    text = text.replace(new RegExp(unquotedKeyRegex.source, 'g'), '"$1":')
    changes.push('keys')
  }

  const trailingCommaRegex = /,\s*([}\]])/g
  if (trailingCommaRegex.test(text)) {
    text = text.replace(new RegExp(trailingCommaRegex.source, 'g'), '$1')
    changes.push('trailing')
  }

  const jsLiterals = /\b(undefined|NaN|Infinity)\b/g
  if (jsLiterals.test(text)) {
    text = text.replace(new RegExp(jsLiterals.source, 'g'), 'null')
    changes.push('literals')
  }

  try {
    JSON.parse(text)
    if (changes.length === 0) return null
    return { fixed: text, changes }
  } catch {
    // continue with structural fixes
  }

  const bracketFixed = fixMissingBrackets(text)
  if (bracketFixed !== text) {
    try {
      JSON.parse(bracketFixed)
      changes.push('brackets')
      return { fixed: bracketFixed, changes }
    } catch {
      text = bracketFixed
    }
  }

  const commaFixed = fixMissingCommas(text)
  if (commaFixed !== text) {
    try {
      JSON.parse(commaFixed)
      changes.push('commas')
      return { fixed: commaFixed, changes }
    } catch {
      text = commaFixed
    }
  }

  const combinedFixed = fixMissingBrackets(text)
  if (combinedFixed !== text) {
    try {
      JSON.parse(combinedFixed)
      changes.push('brackets')
      return { fixed: combinedFixed, changes }
    } catch {
      // unfixable
    }
  }

  const firstBrace = text.indexOf('{')
  const firstBracket = text.indexOf('[')
  if (firstBrace >= 0 || firstBracket >= 0) {
    const startIdx = firstBrace >= 0 && firstBracket >= 0
      ? Math.min(firstBrace, firstBracket)
      : firstBrace >= 0 ? firstBrace : firstBracket
    const extracted = text.substring(startIdx)
    try {
      JSON.parse(extracted)
      changes.push('extract')
      return { fixed: extracted, changes }
    } catch {
      // unfixable
    }
  }

  if (changes.length > 0) {
    return { fixed: text, changes }
  }
  return null
}

function replaceSingleQuotes(text: string): string {
  let result = ''
  let inDouble = false
  let inSingle = false
  let escape = false

  for (let i = 0; i < text.length; i++) {
    const ch = text[i]
    if (escape) {
      result += ch
      escape = false
      continue
    }
    if (ch === '\\') {
      escape = true
      result += ch
      continue
    }
    if (ch === '"' && !inSingle) {
      inDouble = !inDouble
      result += ch
    } else if (ch === "'" && !inDouble) {
      inSingle = !inSingle
      result += '"'
    } else {
      result += ch
    }
  }
  return result
}

function fixMissingBrackets(text: string): string {
  const stack: string[] = []
  let inString = false
  let escape = false

  for (let i = 0; i < text.length; i++) {
    const ch = text[i]
    if (escape) { escape = false; continue }
    if (ch === '\\' && inString) { escape = true; continue }
    if (ch === '"') { inString = !inString; continue }
    if (inString) continue

    if (ch === '{') stack.push('}')
    else if (ch === '[') stack.push(']')
    else if (ch === '}' || ch === ']') {
      if (stack.length > 0 && stack[stack.length - 1] === ch) {
        stack.pop()
      }
    }
  }

  if (stack.length === 0) return text

  let trimmed = text.replace(/[\s,]+$/, '')
  while (stack.length > 0) {
    trimmed += stack.pop()
  }
  return trimmed
}

function fixMissingCommas(text: string): string {
  return text
    .replace(/(["}\]])\s*\n(\s*["{\[])/g, '$1,\n$2')
    .replace(/(true|false|null|\d)\s*\n(\s*["{\[])/g, '$1,\n$2')
}

export interface XmlTreeNode {
  attrs: Record<string, string>
  children: Record<string, XmlTreeNode | XmlTreeNode[]>
  text: string
}

export function parseXmlToTree(text: string): { tag: string; node: XmlTreeNode } | null {
  let doc: Document
  try {
    doc = new DOMParser().parseFromString(text, 'application/xml')
  } catch {
    return null
  }
  const parseError = doc.querySelector('parsererror')
  if (parseError) return null
  const root = doc.documentElement
  if (!root) return null
  return { tag: root.tagName, node: elementToNode(root) }
}

function elementToNode(el: Element): XmlTreeNode {
  const node: XmlTreeNode = { attrs: {}, children: {}, text: '' }
  for (const attr of Array.from(el.attributes)) {
    node.attrs[attr.name] = attr.value
  }
  const childElements = Array.from(el.children)
  if (childElements.length === 0) {
    node.text = (el.textContent || '').trim()
  }
  for (const child of childElements) {
    const converted = elementToNode(child)
    const existing = node.children[child.tagName]
    if (existing === undefined) {
      node.children[child.tagName] = converted
    } else if (Array.isArray(existing)) {
      existing.push(converted)
    } else {
      node.children[child.tagName] = [existing, converted]
    }
  }
  return node
}

export function xmlToTreeValue(node: XmlTreeNode): Record<string, unknown> {
  const value: Record<string, unknown> = {}
  for (const [name, val] of Object.entries(node.attrs)) {
    value[`@${name}`] = val
  }
  for (const [tag, child] of Object.entries(node.children)) {
    value[tag] = Array.isArray(child) ? child.map(xmlToTreeValue) : xmlToTreeValue(child)
  }
  if (Object.keys(value).length === 0 && node.text) {
    value['#text'] = node.text
  }
  return value
}

export interface XmlTreeResult {
  rootTag: string
  value: Record<string, unknown>
}

export function xmlToTreeFromText(text: string): XmlTreeResult | null {
  const parsed = parseXmlToTree(text)
  if (!parsed) return null
  return { rootTag: parsed.tag, value: xmlToTreeValue(parsed.node) }
}

export function formatXmlText(text: string, indent: number | string = 2): string | null {
  let doc: Document
  try {
    doc = new DOMParser().parseFromString(text, 'application/xml')
  } catch {
    return null
  }
  if (doc.querySelector('parsererror') || !doc.documentElement) return null
  const lines: string[] = []
  const decl = text.match(/^\s*<\?xml[^?]*\?>/)
  if (decl) lines.push(decl[0].trim())
  const indentUnit = typeof indent === 'number' ? ' '.repeat(indent) : indent
  serializeElement(doc.documentElement, 0, lines, indentUnit)
  return lines.join('\n')
}

function serializeElement(el: Element, depth: number, lines: string[], indentUnit: string) {
  const indent = indentUnit.repeat(depth)
  const attrs = Array.from(el.attributes)
    .map(a => ` ${a.name}="${a.value.replace(/"/g, '&quot;')}"`)
    .join('')
  const childElements = Array.from(el.children)
  if (childElements.length === 0) {
    const text = (el.textContent || '').trim()
    if (text) {
      lines.push(`${indent}<${el.tagName}${attrs}>${text}</${el.tagName}>`)
    } else {
      lines.push(`${indent}<${el.tagName}${attrs} />`)
    }
    return
  }
  lines.push(`${indent}<${el.tagName}${attrs}>`)
  for (const child of childElements) {
    serializeElement(child, depth + 1, lines, indentUnit)
  }
  lines.push(`${indent}</${el.tagName}>`)
}
