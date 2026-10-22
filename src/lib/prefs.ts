import { readNativeDeviceIdentity } from '@/lib/device/native-settings'
import { nativeGraphql } from '@/lib/api/native-client'

let cache: Map<string, unknown> = new Map()
let writes: Promise<void> = Promise.resolve()
const PREF_KEYS = new Set(['locale', 'clipboard_directions', 'dark_theme'])

async function localData<T>(query: string, variables?: Record<string, unknown>): Promise<T> {
  const result = await nativeGraphql<{ data?: T; errors?: { message: string }[] }>({ query, variables }, 30000)
  if (result.errors?.length) throw new Error(result.errors[0].message)
  if (!result.data) throw new Error('Missing preference response')
  return result.data
}

function persist(query: string, variables: Record<string, unknown>): Promise<void> {
  const current = writes.then(async () => {
    const data = __IS_TAURI__
      ? await localData<{ result: boolean }>(query, variables)
      : await import('@/lib/api/gql-client').then(async ({ gqlFetch }) => {
          const result = await gqlFetch<{ result: boolean }>(query, variables, { dedupe: false })
          if (result.errors?.length) throw new Error(result.errors[0].message)
          return result.data
        })
    if (!data.result) throw new Error('Preference write refused')
  })
  writes = current.catch((error) => { console.error('Unable to save preferences', error) })
  return current
}

export async function preload(): Promise<void> {
  if (!__IS_TAURI__) return
  await writes
  const [{ prefs: snapshot }, identity] = await Promise.all([
    localData<{ prefs: Record<string, unknown> }>('query { prefs }'),
    readNativeDeviceIdentity(),
  ])
  if (!snapshot || typeof snapshot !== 'object' || Array.isArray(snapshot)) {
    throw new Error('Invalid preference snapshot')
  }
  cache = new Map(Object.entries(snapshot))
  cache.set('client_id', identity.clientId)
}

export function get<T>(key: string, fallback: T): T {
  if (PREF_KEYS.has(key) || (__IS_TAURI__ && key === 'client_id')) {
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
  if (PREF_KEYS.has(key)) {
    return persist('mutation($key: String!, $value: JSON!) { result: setPref(key: $key, value: $value) }', { key, value }).then(() => { cache.set(key, value) })
  }
  localStorage.setItem(key, typeof value === 'string' ? value : JSON.stringify(value))
  return Promise.resolve()
}

export function remove(key: string): Promise<void> {
  if (PREF_KEYS.has(key)) {
    return persist('mutation($key: String!) { result: removePref(key: $key) }', { key }).then(() => { cache.delete(key) })
  }
  localStorage.removeItem(key)
  return Promise.resolve()
}

export function clearWebPreferences(): void {
  localStorage.clear()
}

export async function applySnapshot(snapshot: Record<string, unknown>): Promise<void> {
  for (const key of PREF_KEYS) {
    if (key in snapshot) cache.set(key, snapshot[key])
    else cache.delete(key)
  }
  const [{ changeColor }, { loadLocaleMessages, syncLocaleFromPrefs }] = await Promise.all([
    import('@/lib/theme'), import('@/plugins/i18n'),
  ])
  changeColor()
  const locale = get('locale', '')
  if (locale) await loadLocaleMessages(locale)
  syncLocaleFromPrefs()
}
