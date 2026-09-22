export interface ExtractedDoc {
  key: string
  file: string
  text: string
}

export interface ExtractWarning {
  file: string
  line: number
  name: string
  expr: string
}

export interface ExtractResult {
  docs: ExtractedDoc[]
  warnings: ExtractWarning[]
  errors: string[]
}

interface RawTemplate {
  parts: string[]
  holes: string[]
  offset: number
  line: number
}

const IDENT = /^[A-Za-z_$][\w$]*$/
const OP_KEYWORD = /^(query|mutation|fragment|subscription)\b/

export function extractDocuments(files: Map<string, string>): ExtractResult {
  const scanned = new Map<string, RawTemplate[]>()
  const bindings = new Map<string, Map<string, RawTemplate>>()
  const importsByFile = new Map<string, Map<string, { file: string; name: string }>>()
  const resolvedCache = new Map<string, string>()

  const docs: ExtractedDoc[] = []
  const warnings: ExtractWarning[] = []
  const errors: string[] = []

  for (const [file, text] of files) {
    if (!/\.(ts|vue)$/.test(file) || file.startsWith('lib/api/graphql/')) continue
    scanned.set(file, scanTemplates(text))
  }

  for (const [file, templates] of scanned) {
    for (const tpl of templates) {
      const staticText = tpl.parts.join('${…}')
      if (!OP_KEYWORD.test(staticText.trimStart()) || !looksLikeDocument(staticText)) continue
      const unresolved = tpl.holes.find((h) => IDENT.test(h) === false || resolveIdentifier(h, file, new Set()) === undefined)
      if (unresolved !== undefined) {
        warnings.push({ file, line: tpl.line, name: bindingNameAt(file, tpl.offset) ?? '', expr: unresolved })
        continue
      }
      const text = resolve(tpl, file, new Set())
      if (text === undefined) continue
      docs.push({ key: bindingNameAt(file, tpl.offset) ?? `${file}:${tpl.line}`, file, text })
    }
  }

  return { docs, warnings, errors }

  function resolve(tpl: RawTemplate, file: string, stack: Set<string>): string | undefined {
    let out = ''
    for (let i = 0; i < tpl.parts.length; i++) {
      out += tpl.parts[i]
      if (i >= tpl.holes.length) break
      const hole = tpl.holes[i]
      const value = resolveIdentifier(hole, file, stack)
      if (value === undefined) return undefined
      out += value
    }
    return out
  }

  function resolveIdentifier(name: string, file: string, stack: Set<string>): string | undefined {
    const cacheKey = `${file}::${name}`
    const cached = resolvedCache.get(cacheKey)
    if (cached !== undefined) return cached

    let tpl = bindingsOf(file).get(name)
    let tplFile = file
    if (!tpl) {
      const imp = importsOf(file).get(name)
      if (!imp || !files.has(imp.file)) return undefined
      tplFile = imp.file
      tpl = bindingsOf(tplFile).get(imp.name)
      if (!tpl) return undefined
    }

    const frame = `${tplFile}#${tpl.offset}`
    if (stack.has(frame)) {
      errors.push(`${file}: circular interpolation of '\${${name}}'`)
      return undefined
    }
    stack.add(frame)
    const text = resolve(tpl, tplFile, stack)
    stack.delete(frame)
    if (text === undefined) return undefined
    resolvedCache.set(cacheKey, text)
    return text
  }

  function bindingsOf(file: string): Map<string, RawTemplate> {
    const cached = bindings.get(file)
    if (cached) return cached
    const map = new Map<string, RawTemplate>()
    for (const tpl of scanned.get(file) ?? []) {
      const name = bindingNameAt(file, tpl.offset)
      if (name) map.set(name, tpl)
    }
    bindings.set(file, map)
    return map
  }

  function importsOf(file: string): Map<string, { file: string; name: string }> {
    const cached = importsByFile.get(file)
    if (cached) return cached
    const map = new Map<string, { file: string; name: string }>()
    const text = files.get(file) ?? ''
    const re = /import\s+(?:type\s+)?\{([^}]*)\}\s+from\s+['"]([^'"]+)['"]/g
    let m: RegExpExecArray | null
    while ((m = re.exec(text))) {
      const source = resolveModuleFile(m[2], file)
      if (!source) continue
      for (const spec of m[1].split(',')) {
        const parts = spec.trim().split(/\s+as\s+/)
        const imported = parts[0].trim()
        if (!imported) continue
        const local = parts.length === 2 ? parts[1].trim() : imported
        map.set(local, { file: source, name: imported })
      }
    }
    importsByFile.set(file, map)
    return map
  }

  function bindingNameAt(file: string, offset: number): string | undefined {
    const text = files.get(file)
    if (!text) return undefined
    const before = text.slice(0, offset)
    const direct = /(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*$/.exec(before)
    if (direct) return direct[1]
    const fn = /function\s+([A-Za-z_$][\w$]*)/g
    let fnName: string | undefined
    let m: RegExpExecArray | null
    while ((m = fn.exec(before))) fnName = m[1]
    if (fnName) return fnName
    const binding = /(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*[=(]/g
    let last: string | undefined
    while ((m = binding.exec(before))) last = m[1]
    return last
  }

  function resolveModuleFile(spec: string, fromFile: string): string | undefined {
    let base: string
    if (spec.startsWith('@/')) base = spec.slice(2)
    else if (spec.startsWith('.')) {
      const dir = fromFile.includes('/') ? fromFile.slice(0, fromFile.lastIndexOf('/')) : ''
      const segments = dir.split('/').filter((s) => s !== '' && s !== '.')
      for (const seg of spec.split('/')) {
        if (seg === '' || seg === '.') continue
        if (seg === '..') segments.pop()
        else segments.push(seg)
      }
      base = segments.join('/')
    } else return undefined
    for (const candidate of [base, `${base}.ts`, `${base}.vue`, `${base}/index.ts`]) {
      if (files.has(candidate)) return candidate
    }
    return undefined
  }
}

function looksLikeDocument(text: string): boolean {
  const trimmed = text.trimStart()
  const firstLine = trimmed.split('\n')[0] ?? ''
  if (/\{\s*$/.test(firstLine) || /\)\s*\{/.test(firstLine)) return true
  return /^(query|mutation|fragment|subscription)\b[^{}]*\{[^{}]*\}\s*$/.test(trimmed)
}

export function scanTemplates(text: string): RawTemplate[] {
  const out: RawTemplate[] = []
  let i = 0
  let line = 1
  while (i < text.length) {
    const c = text[i]
    if (c === '\n') line++
    if (c === '/' && text[i + 1] === '/') {
      while (i < text.length && text[i] !== '\n') i++
      continue
    }
    if (c === '/' && text[i + 1] === '*') {
      i += 2
      while (i < text.length && !(text[i] === '*' && text[i + 1] === '/')) {
        if (text[i] === '\n') line++
        i++
      }
      i += 2
      continue
    }
    if (c === "'" || c === '"') {
      const quote = c
      i++
      while (i < text.length && text[i] !== quote) {
        if (text[i] === '\\') i++
        if (text[i] === '\n') line++
        i++
      }
      i++
      continue
    }
    if (c !== '`') {
      i++
      continue
    }
    const startLine = line
    const offset = i
    i++
    const parts: string[] = ['']
    const holes: string[] = []
    while (i < text.length && text[i] !== '`') {
      if (text[i] === '\\') {
        parts[parts.length - 1] += unescapeTemplateChar(text[i + 1])
        i += 2
        continue
      }
      if (text[i] === '\n') line++
      if (text[i] === '$' && text[i + 1] === '{') {
        i += 2
        const holeStart = i
        let depth = 1
        while (i < text.length && depth > 0) {
          const h = text[i]
          if (h === '{') depth++
          else if (h === '}') depth--
          else if (h === "'" || h === '"' || h === '`') {
            i++
            while (i < text.length && text[i] !== h) {
              if (text[i] === '\\') i++
              i++
            }
          } else if (h === '\n') {
            line++
          }
          if (depth > 0) i++
        }
        holes.push(text.slice(holeStart, i).trim())
        i++
        parts.push('')
        continue
      }
      parts[parts.length - 1] += text[i]
      i++
    }
    i++
    out.push({ parts, holes, offset, line: startLine })
  }
  return out
}

function unescapeTemplateChar(c: string | undefined): string {
  if (c === 'n') return '\n'
  if (c === 't') return '\t'
  if (c === '`') return '`'
  if (c === '$') return '$'
  if (c === '\\') return '\\'
  return c ?? ''
}
