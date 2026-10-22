import { gqlFetch } from '@/lib/api/gql-client'

export async function readSavedValue<T>(key: string, fallback: T): Promise<T> {
  const result = await gqlFetch<{ savedValue: T | null }>(
    'query($key: String!) { savedValue(key: $key) }', { key }, { fresh: true },
  )
  if (result.errors?.length) throw new Error(result.errors[0].message)
  return result.data.savedValue ?? fallback
}

export async function saveValue(key: string, value: unknown): Promise<void> {
  const result = await gqlFetch<{ setSavedValue: boolean }>(
    'mutation($key: String!, $value: JSON!) { setSavedValue(key: $key, value: $value) }',
    { key, value }, { dedupe: false },
  )
  if (result.errors?.length) throw new Error(result.errors[0].message)
}
