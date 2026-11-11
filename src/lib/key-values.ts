import { gqlFetch } from '@/lib/api/gql-client'

export async function readKeyValue<T>(key: string, fallback: T): Promise<T> {
  const result = await gqlFetch<{ keyValue: T | null }>(
    'query($key: String!) { keyValue(key: $key) }', { key }, { fresh: true },
  )
  if (result.errors?.length) throw new Error(result.errors[0].message)
  return result.data.keyValue ?? fallback
}

export async function setKeyValue(key: string, value: unknown): Promise<void> {
  const result = await gqlFetch<{ setKeyValue: boolean }>(
    'mutation($key: String!, $value: JSON!) { setKeyValue(key: $key, value: $value) }',
    { key, value }, { dedupe: false },
  )
  if (result.errors?.length) throw new Error(result.errors[0].message)
}
