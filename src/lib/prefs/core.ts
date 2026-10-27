import { bindGraphqlClient, getGraphqlContextKey, gqlFetch } from '@/lib/api/gql-client'

let cache = new Map<string, unknown>()
let cacheContext: string | undefined
let writes: Promise<void> = Promise.resolve()

function currentCache(): Map<string, unknown> {
  const context = getGraphqlContextKey()
  if (context !== cacheContext) {
    cache = new Map()
    cacheContext = context
  }
  return cache
}

export async function preload(): Promise<void> {
  await writes
  const result = await gqlFetch<{ prefs: Record<string, unknown> }>('query { prefs }', undefined, { fresh: true })
  if (result.errors?.length) throw new Error(result.errors[0].message)
  const snapshot = result.data.prefs
  if (!snapshot || typeof snapshot !== 'object' || Array.isArray(snapshot)) {
    throw new Error('Invalid preference snapshot')
  }
  applySnapshot(snapshot)
}

export function get<T>(key: string, fallback: T): T {
  const values = currentCache()
  return values.has(key) ? (values.get(key) as T) : fallback
}

export function set(key: string, value: unknown): Promise<void> {
  const context = getGraphqlContextKey()
  const request = bindGraphqlClient()
  const current = writes.then(async () => {
    const result = await request<{ result: boolean }>(
      'mutation($key: String!, $value: JSON!) { result: setPref(key: $key, value: $value) }',
      { key, value },
    )
    if (result.errors?.length) throw new Error(result.errors[0].message)
    if (!result.data.result) throw new Error('Preference write refused')
    if (context === getGraphqlContextKey()) currentCache().set(key, value)
  })
  writes = current.catch((error) => { console.error('Unable to save preferences', error) })
  return current
}

export function applySnapshot(snapshot: Record<string, unknown>): void {
  const values = currentCache()
  values.clear()
  for (const [key, value] of Object.entries(snapshot)) values.set(key, value)
}
