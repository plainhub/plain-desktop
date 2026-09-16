export interface TreeRowOpen {
  kind: 'open'
  rowKey: string
  path: string
  depth: number
  key: string
  showKey: boolean
  isArray: boolean
  childCount: number
  isCollapsed: boolean
  trailingComma: boolean
  value: unknown
}

export interface TreeRowClose {
  kind: 'close'
  rowKey: string
  path: string
  depth: number
  isArray: boolean
  trailingComma: boolean
}

export interface TreeRowValue {
  kind: 'value'
  rowKey: string
  path: string
  depth: number
  key: string
  showKey: boolean
  value: unknown
  trailingComma: boolean
}

export type TreeRow = TreeRowOpen | TreeRowClose | TreeRowValue

export interface CollapseState {
  expanded: Set<string>
  collapsed: Set<string>
  expandDepth: number
}

export function needsBracketNotation(key: string): boolean {
  return /[.\s[\]()@,;:!'"\\]/.test(key)
}

export function buildJsonPath(parentPath: string, key: string): string {
  return needsBracketNotation(key) ? `${parentPath}["${key}"]` : `${parentPath}.${key}`
}

export function isNodeCollapsed(path: string, depth: number, state: CollapseState): boolean {
  if (state.expanded.has(path)) return false
  if (state.collapsed.has(path)) return true
  return depth >= state.expandDepth
}

export function buildRows(value: unknown, state: CollapseState): TreeRow[] {
  const rows: TreeRow[] = []
  build(value, '$', 0, '', false, false, rows, state)
  return rows
}

function build(
  value: unknown,
  path: string,
  depth: number,
  key: string,
  showKey: boolean,
  trailingComma: boolean,
  rows: TreeRow[],
  state: CollapseState,
) {
  const isArray = Array.isArray(value)
  const isObject = value !== null && typeof value === 'object' && !isArray
  if (!isArray && !isObject) {
    rows.push({ kind: 'value', rowKey: path, path, depth, key, showKey, value, trailingComma })
    return
  }
  const collapsed = isNodeCollapsed(path, depth, state)
  const childCount = isArray ? (value as unknown[]).length : Object.keys(value as object).length
  rows.push({
    kind: 'open',
    rowKey: `${path}:o`,
    path,
    depth,
    key,
    showKey,
    isArray,
    childCount,
    isCollapsed: collapsed,
    trailingComma: collapsed ? trailingComma : false,
    value,
  })
  if (collapsed) return
  if (isArray) {
    const arr = value as unknown[]
    for (let i = 0; i < arr.length; i++) {
      build(arr[i], `${path}[${i}]`, depth + 1, String(i), false, i < arr.length - 1, rows, state)
    }
  } else {
    const record = value as Record<string, unknown>
    const keys = Object.keys(record)
    for (let i = 0; i < keys.length; i++) {
      const k = keys[i]
      build(record[k], buildJsonPath(path, k), depth + 1, k, true, i < keys.length - 1, rows, state)
    }
  }
  rows.push({ kind: 'close', rowKey: `${path}:c`, path, depth, isArray, trailingComma })
}
