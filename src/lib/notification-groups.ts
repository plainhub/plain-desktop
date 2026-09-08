import type { INotification } from './interfaces'

/** Prepends a new notification, or replaces the existing one in place. */
export function upsertNotification(items: INotification[], n: INotification): INotification[] {
  const idx = items.findIndex((it) => it.id === n.id)
  if (idx === -1) return [n, ...items]
  const next = items.slice()
  next[idx] = n
  return next
}

export function removeNotification(items: INotification[], id: string): INotification[] {
  return items.filter((it) => it.id !== id)
}
