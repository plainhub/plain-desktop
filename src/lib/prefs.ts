/**
 * Unified preferences store.
 *
 * On Tauri: backed by @tauri-apps/plugin-store ("prefs.json").
 *   - Call `preload()` once during app bootstrap (before mounting) to populate
 *     the in-memory cache. Subsequent `get()` calls are synchronous.
 *   - `set()` updates the cache immediately and fire-and-forgets the async save.
 *
 * On web: backed by localStorage (unchanged behaviour).
 */

let cache: Map<string, unknown> = new Map()
 
let storeInstance: any = null

export async function preload(): Promise<void> {
  if (!__IS_TAURI__) return
  const { load } = await import('@tauri-apps/plugin-store')
  const store = await load('prefs.json', { autoSave: false, defaults: {} })
  storeInstance = store
  const entries = (await store.entries()) as Array<[string, unknown]>
  for (const [k, v] of entries) {
    cache.set(k, v)
  }
}

export function get<T>(key: string, fallback: T): T {
  if (__IS_TAURI__) {
    return cache.has(key) ? (cache.get(key) as T) : fallback
  }
  const raw = localStorage.getItem(key)
  if (raw === null) return fallback
  if (typeof fallback === 'string') return raw as unknown as T
  try { return JSON.parse(raw) as T } catch { return raw as unknown as T }
}

export function set(key: string, value: unknown): void {
  if (__IS_TAURI__) {
    cache.set(key, value)
    if (storeInstance) {
      storeInstance.set(key, value).then(() => storeInstance!.save()).catch(() => {})
    }
    return
  }
  if (typeof value === 'string') {
    localStorage.setItem(key, value)
  } else {
    localStorage.setItem(key, JSON.stringify(value))
  }
}

export function remove(key: string): void {
  if (__IS_TAURI__) {
    cache.delete(key)
    if (storeInstance) {
      storeInstance.delete(key).then(() => storeInstance!.save()).catch(() => {})
    }
    return
  }
  localStorage.removeItem(key)
}

export function clear(): void {
  if (__IS_TAURI__) {
    cache.clear()
    if (storeInstance) {
      storeInstance.clear().then(() => storeInstance!.save()).catch(() => {})
    }
    return
  }
  localStorage.clear()
}
