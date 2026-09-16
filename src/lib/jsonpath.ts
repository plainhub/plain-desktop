export interface PathSuggestion {
  path: string
  type: string
}

export function needsBracketNotation(key: string): boolean {
  return /[.\s[\]()@,;:!'"\\]/.test(key)
}

export function jsonPathSegment(key: string): string {
  return needsBracketNotation(key) ? `["${key}"]` : `.${key}`
}

export function buildJsonPath(parentPath: string, key: string): string {
  return needsBracketNotation(key) ? `${parentPath}["${key}"]` : `${parentPath}.${key}`
}

function parsePathSegments(path: string): string[] {
  const normalized = path.replace(/^\$/, '')
  if (!normalized) return []
  const segments: string[] = []
  let i = 0
  while (i < normalized.length) {
    if (normalized[i] === '[') {
      if (normalized[i + 1] === '?') {
        let depth = 0
        let end = i
        for (; end < normalized.length; end++) {
          if (normalized[end] === '[') depth++
          else if (normalized[end] === ']') { depth--; if (depth === 0) { end++; break } }
        }
        segments.push(normalized.slice(i, end))
        i = end
        if (normalized[i] === '.') i++
        continue
      }
      const quotedMatch = normalized.slice(i).match(/^\["([^"]*?)"\]/)
      if (quotedMatch) {
        segments.push(quotedMatch[1])
        i += quotedMatch[0].length
        if (normalized[i] === '.') i++
        continue
      }
      const bracketMatch = normalized.slice(i).match(/^\[[^\]]*\]/)
      if (bracketMatch) {
        segments.push(bracketMatch[0])
        i += bracketMatch[0].length
        if (normalized[i] === '.') i++
        continue
      }
      i++
      continue
    }
    if (normalized[i] === '.') {
      if (normalized[i + 1] === '.') {
        segments.push('')
        i += 2
        continue
      }
      i++
      continue
    }
    let end = i
    while (end < normalized.length && normalized[end] !== '.' && normalized[end] !== '[') end++
    if (end > i) segments.push(normalized.slice(i, end))
    i = end
  }
  return segments
}

export function evaluateJsonPath(data: unknown, path: string): unknown[] {
  if (path === '$') return [data]
  const results: unknown[] = []
  const segments = parsePathSegments(path)
  if (!segments.length) return results

  function query(current: unknown, segs: string[]): void {
    if (segs.length === 0) { results.push(current); return }
    const [seg, ...rest] = segs
    if (seg === '') {
      query(current, rest)
      if (typeof current === 'object' && current !== null) {
        if (Array.isArray(current)) current.forEach(item => query(item, segs))
        else Object.values(current as Record<string, unknown>).forEach(val => query(val, segs))
      }
      return
    }
    if (seg === '*') {
      if (Array.isArray(current)) current.forEach(item => query(item, rest))
      else if (typeof current === 'object' && current !== null) Object.values(current as Record<string, unknown>).forEach(val => query(val, rest))
      return
    }
    const arrayMatch = seg.match(/^\[(\d+)\]$/)
    if (arrayMatch && Array.isArray(current)) { const idx = parseInt(arrayMatch[1]); if (idx < current.length) query(current[idx], rest); return }
    if (seg === '[*]' && Array.isArray(current)) { current.forEach(item => query(item, rest)); return }
    const filterMatch = seg.match(/^\[\?\(@(\.[\w]+|\['[^']+'\])\s*(==|!=|<|>|<=|>=)\s*(.+)\)\]$/)
    if (filterMatch && Array.isArray(current)) {
      const rawKey = filterMatch[1].startsWith("['") ? filterMatch[1].slice(2, -2) : filterMatch[1].slice(1)
      const op = filterMatch[2]
      const rawVal = filterMatch[3].trim()
      for (const item of current) {
        if (typeof item === 'object' && item !== null && !Array.isArray(item)) {
          const itemVal = (item as Record<string, unknown>)[rawKey]
          if (matchFilter(itemVal, op, rawVal)) query(item, rest)
        }
      }
      return
    }
    if (typeof current === 'object' && current !== null && !Array.isArray(current)) {
      const obj = current as Record<string, unknown>
      if (seg in obj) query(obj[seg], rest)
    }
  }

  query(data, segments)
  return results
}

function matchFilter(itemVal: unknown, op: string, rawVal: string): boolean {
  let compareVal: unknown
  if ((rawVal.startsWith('"') && rawVal.endsWith('"')) || (rawVal.startsWith("'") && rawVal.endsWith("'"))) {
    compareVal = rawVal.slice(1, -1)
  } else if (rawVal === 'true') compareVal = true
  else if (rawVal === 'false') compareVal = false
  else if (rawVal === 'null') compareVal = null
  else compareVal = Number(rawVal)

  switch (op) {
    case '==': return itemVal == compareVal
    case '!=': return itemVal != compareVal
    case '<': return typeof itemVal === 'number' && typeof compareVal === 'number' && itemVal < compareVal
    case '>': return typeof itemVal === 'number' && typeof compareVal === 'number' && itemVal > compareVal
    case '<=': return typeof itemVal === 'number' && typeof compareVal === 'number' && itemVal <= compareVal
    case '>=': return typeof itemVal === 'number' && typeof compareVal === 'number' && itemVal >= compareVal
    default: return false
  }
}

function childPath(parentPath: string, key: string, isArray: boolean, index: number): string {
  return isArray ? `${parentPath}[${index}]` : buildJsonPath(parentPath, key)
}

export function collectJsonPathMatches(data: unknown, path: string): string[] {
  if (path === '$') return ['$']
  const paths: string[] = []
  const segments = parsePathSegments(path)
  if (!segments.length) return paths

  function query(current: unknown, segs: string[], currentPath: string): void {
    if (segs.length === 0) { paths.push(currentPath); return }
    const [seg, ...rest] = segs
    if (seg === '') {
      query(current, rest, currentPath)
      if (Array.isArray(current)) current.forEach((item, i) => query(item, segs, `${currentPath}[${i}]`))
      else if (typeof current === 'object' && current !== null) {
        Object.entries(current as Record<string, unknown>).forEach(([k, v]) => query(v, segs, buildJsonPath(currentPath, k)))
      }
      return
    }
    if (seg === '*') {
      if (Array.isArray(current)) current.forEach((item, i) => query(item, rest, `${currentPath}[${i}]`))
      else if (typeof current === 'object' && current !== null) {
        Object.entries(current as Record<string, unknown>).forEach(([k, v]) => query(v, rest, buildJsonPath(currentPath, k)))
      }
      return
    }
    const arrayMatch = seg.match(/^\[(\d+)\]$/)
    if (arrayMatch && Array.isArray(current)) {
      const idx = parseInt(arrayMatch[1])
      if (idx < current.length) query(current[idx], rest, `${currentPath}[${idx}]`)
      return
    }
    if (seg === '[*]' && Array.isArray(current)) {
      current.forEach((item, i) => query(item, rest, `${currentPath}[${i}]`))
      return
    }
    const filterMatch = seg.match(/^\[\?\(@(\.[\w]+|\['[^']+'\])\s*(==|!=|<|>|<=|>=)\s*(.+)\)\]$/)
    if (filterMatch && Array.isArray(current)) {
      const rawKey = filterMatch[1].startsWith("['") ? filterMatch[1].slice(2, -2) : filterMatch[1].slice(1)
      const op = filterMatch[2]
      const rawVal = filterMatch[3].trim()
      current.forEach((item, i) => {
        if (typeof item === 'object' && item !== null && !Array.isArray(item)) {
          const itemVal = (item as Record<string, unknown>)[rawKey]
          if (matchFilter(itemVal, op, rawVal)) query(item, rest, `${currentPath}[${i}]`)
        }
      })
      return
    }
    if (typeof current === 'object' && current !== null && !Array.isArray(current)) {
      const obj = current as Record<string, unknown>
      if (seg in obj) query(obj[seg], rest, childPath(currentPath, seg, false, 0))
    }
  }

  query(data, segments, '$')
  return paths
}

export function generateSuggestions(data: unknown, expr: string): PathSuggestion[] {
  const results: PathSuggestion[] = []
  let lastSepIdx = -1
  let inBracketQuote = false
  for (let i = 0; i < expr.length; i++) {
    if (expr[i] === '[' && expr[i + 1] === '"') { inBracketQuote = true; continue }
    if (inBracketQuote && expr[i] === '"' && expr[i + 1] === ']') { inBracketQuote = false; i++; continue }
    if (!inBracketQuote && (expr[i] === '.' || expr[i] === '[')) lastSepIdx = i
  }

  const parentPath = lastSepIdx > 0 ? expr.substring(0, lastSepIdx) : '$'
  const partial = lastSepIdx > 0 ? expr.substring(lastSepIdx + 1) : expr === '$' ? '' : expr.substring(1)

  let current: unknown = data
  if (parentPath !== '$') {
    const segments = parsePathSegments(parentPath)
    for (const seg of segments) {
      const arrayMatch = seg.match(/^\[(\d+)\]$/)
      if (arrayMatch) {
        if (Array.isArray(current)) current = current[parseInt(arrayMatch[1])]
        else return results
      } else if (typeof current === 'object' && current !== null && !Array.isArray(current)) {
        current = (current as Record<string, unknown>)[seg]
      } else { return results }
      if (current === undefined) return results
    }
  }

  if (typeof current !== 'object' || current === null) return results

  if (Array.isArray(current)) {
    const prefix = parentPath === '$' && !expr.includes('.') ? '$' : parentPath
    for (let i = 0; i < Math.min(current.length, 3); i++) {
      const p = `${prefix}[${i}]`
      const itemType = current[i] === null ? 'null' : Array.isArray(current[i]) ? 'array' : typeof current[i]
      if (!partial || `[${i}]`.startsWith(partial) || p.endsWith(partial)) {
        results.push({ path: p, type: itemType })
      }
    }
    if (current.length > 0) results.push({ path: `${prefix}[*]`, type: 'wildcard' })
    generateArrayFilterSuggestions(current, prefix, partial, results)
  } else {
    const keys = Object.keys(current as Record<string, unknown>)
    for (const key of keys) {
      if (!partial || key.toLowerCase().startsWith(partial.toLowerCase())) {
        const val = (current as Record<string, unknown>)[key]
        const valType = val === null ? 'null' : Array.isArray(val) ? 'array' : typeof val
        const prefix = parentPath === '$' && !expr.includes('.') && !expr.includes('[') ? '$' : parentPath
        results.push({ path: buildJsonPath(prefix, key), type: valType })
      }
    }
  }
  return results
}

function generateArrayFilterSuggestions(arr: unknown[], prefix: string, partial: string, results: PathSuggestion[]) {
  if (arr.length === 0) return
  const firstObj = arr.find(item => typeof item === 'object' && item !== null && !Array.isArray(item)) as Record<string, unknown> | undefined
  if (!firstObj) return

  const filterKeys = Object.keys(firstObj).filter((key) => {
    const v = firstObj[key]
    return v !== null && v !== undefined && typeof v !== 'object'
  })

  for (const key of filterKeys.slice(0, 8)) {
    const samples = new Set<string>()
    for (let i = 0; i < Math.min(arr.length, 5); i++) {
      const item = arr[i] as Record<string, unknown>
      if (item && typeof item === 'object' && key in item) {
        const v = item[key]
        if (v !== null && v !== undefined && typeof v !== 'object') samples.add(String(v))
      }
      if (samples.size >= 3) break
    }
    const sampleVal = [...samples][0]
    if (sampleVal === undefined) continue
    const keySeg = needsBracketNotation(key) ? `['${key}']` : `.${key}`
    const valLiteral = typeof firstObj[key] === 'string' ? `"${sampleVal}"` : sampleVal
    const filterPath = `${prefix}[?(@${keySeg}==${valLiteral})]`
    const filterPartial = partial?.replace(/^\[?\??/, '') || ''
    if (!filterPartial || key.toLowerCase().includes(filterPartial.toLowerCase()) || filterPath.includes(filterPartial)) {
      results.push({ path: filterPath, type: 'filter' })
    }
  }
}
