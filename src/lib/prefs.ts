import { invoke } from '@tauri-apps/api/core'
import { nativeGraphql } from '@/lib/api/native-client'

let cache: Map<string, unknown> = new Map()
let writes: Promise<void> = Promise.resolve()

async function localData<T>(query: string, variables?: Record<string, unknown>): Promise<T> {
  const result = await nativeGraphql<{ data?: T; errors?: { message: string }[] }>({ query, variables }, 30000)
  if (result.errors?.length) throw new Error(result.errors[0].message)
  if (!result.data) throw new Error('Missing preference response')
  return result.data
}

function persist(query: string, variables: Record<string, unknown>): Promise<void> {
  const current = writes.then(async () => {
    const data = await localData<{ result?: { affectedCount: number } }>(query, variables)
    if (typeof data.result?.affectedCount !== 'number') throw new Error('Invalid preference write response')
  })
  writes = current.catch((error) => { console.error('Unable to save preferences', error) })
  return current
}

export async function preload(): Promise<void> {
  if (!__IS_TAURI__) return
  await writes
  const snapshot = await invoke<Record<string, unknown>>('native_system_preferences')
  if (!snapshot || typeof snapshot !== 'object' || Array.isArray(snapshot)) {
    throw new Error('Invalid preference snapshot')
  }
  cache = new Map(Object.entries(snapshot))
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

export function set(key: string, value: unknown): Promise<void> {
  if (__IS_TAURI__) {
    cache.set(key, value)
    return persist('mutation($key: String!, $value: JSON!) { result: setSystemPref(key: $key, value: $value) { affectedCount } }', { key, value })
  }
  localStorage.setItem(key, typeof value === 'string' ? value : JSON.stringify(value))
  return Promise.resolve()
}

export function remove(key: string): Promise<void> {
  if (__IS_TAURI__) {
    cache.delete(key)
    return persist('mutation($key: String!) { result: removeSystemPref(key: $key) { affectedCount } }', { key })
  }
  localStorage.removeItem(key)
  return Promise.resolve()
}

export function clearWebPreferences(): void {
  localStorage.clear()
}
