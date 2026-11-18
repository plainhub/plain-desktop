export function isTrashedMediaPath(path: string | undefined): boolean {
  const components = path?.split(/[/\\]/) ?? []
  return components.slice(0, -1).includes('.plain-trash') || !!components.at(-1)?.startsWith('.trashed-')
}
