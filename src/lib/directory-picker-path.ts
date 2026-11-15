const windowsDrivePath = /^[a-z]:[/\\]/i

export function normalizeDirectoryPath(path: string): string {
  if (windowsDrivePath.test(path)) {
    const rest = path.slice(2).replace(/[/\\]+/g, '/').replace(/\/$/, '')
    return path[0].toUpperCase() + ':' + (rest || '/')
  }
  return path.replace(/\/+$/, '') || (path.startsWith('/') ? '/' : '')
}

export function directoryPathKey(path: string): string {
  const normal = normalizeDirectoryPath(path)
  return windowsDrivePath.test(normal) ? normal.toLowerCase() : normal
}

export function directoryWithinRoot(path: string, root: string): boolean {
  const candidate = directoryPathKey(path)
  const base = directoryPathKey(root)
  if (!(base.startsWith('/') || windowsDrivePath.test(base))) return false
  if (/(^|\/)\.{1,2}(\/|$)/.test(candidate)) return false
  return candidate === base || candidate.startsWith(base.endsWith('/') ? base : base + '/')
}
