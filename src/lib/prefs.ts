/**
 * Unified preferences store.
 *
 * On Tauri: backed by the Rust-side plain-rs Prefs engine
 * (`<app_data_dir>/prefs.json` — the same file the local API server
 * uses) through the `prefs_*` IPC commands.
 *   - Call `preload()` once during app bootstrap (before mounting) to
 *     populate the in-memory cache. Subsequent `get()` calls are
 *     synchronous.
 *   - `set()` updates the cache immediately and fire-and-forgets the
 *     async save.
 *
 * On web: backed by localStorage (unchanged behaviour).
 */
import { invoke } from '@tauri-apps/api/core'

let cache: Map<string, unknown> = new Map()

export async function preload(): Promise<void> {
  if (!__IS_TAURI__) return
  const entries = (await invoke('prefs_get_all')) as Array<[string, unknown]>
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
  try {
    return JSON.parse(raw) as T
  } catch {
    return raw as unknown as T
  }
}

export function set(key: string, value: unknown): void {
  if (__IS_TAURI__) {
    cache.set(key, value)
    invoke('prefs_set', { key, value }).catch(() => {})
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
    invoke('prefs_remove', { key }).catch(() => {})
    return
  }
  localStorage.removeItem(key)
}

export function clear(): void {
  if (__IS_TAURI__) {
    cache.clear()
    invoke('prefs_clear').catch(() => {})
    return
  }
  localStorage.clear()
}
