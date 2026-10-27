export function get<T>(key: string, fallback: T): T {
  const raw = localStorage.getItem(key)
  if (raw === null) return fallback
  if (typeof fallback === 'string') return raw as unknown as T
  try {
    return JSON.parse(raw) as T
  } catch {
    return raw as unknown as T
  }
}

export function set(key: string, value: unknown): void {
  localStorage.setItem(key, typeof value === 'string' ? value : JSON.stringify(value))
}

export function remove(key: string): void {
  localStorage.removeItem(key)
}

export function clear(keep: readonly string[] = []): void {
  const saved = keep.map((key) => [key, localStorage.getItem(key)] as const)
  localStorage.clear()
  for (const [key, value] of saved) {
    if (value !== null) localStorage.setItem(key, value)
  }
}
