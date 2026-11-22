import { createPinia } from 'pinia'
import { expect, it, vi } from 'vitest'
import { useSmsStore } from '@/stores/sms'
import { parseDocument } from '@/lib/api/graphql/parser'

const { gqlFetch, showActionError } = vi.hoisted(() => ({ gqlFetch: vi.fn(), showActionError: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {} }))
vi.mock('@/hooks/show-action-error', () => ({ showActionError }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({}) }))
vi.mock('@/lib/api/cache', () => ({ deleteCachedByPrefix: vi.fn(), getCached: vi.fn(), setCached: vi.fn() }))

it.each([
  ['archiveConversations', 'archiveSmsConversation'],
  ['unarchiveConversations', 'unarchiveSmsConversation'],
] as const)('matches the Core ID argument when calling %s', async (action, field) => {
  gqlFetch.mockReset()
  showActionError.mockClear()
  const id = 'fixture-thread'
  gqlFetch.mockImplementation(async (document: string, variables: Record<string, string>) => {
    const [operation] = parseDocument(document).operations
    if (operation.type !== 'mutation') return { data: { smsConversations: [], smsConversationCount: 0 } }

    expect(operation.name).toBe(field)
    expect(operation.selections[0]).toMatchObject({ name: field, args: [{ name: 'id', value: { kind: 'variable', name: 'id' } }] })
    expect(variables).toEqual({ id })
    const definition = operation.variables.find(variable => variable.name === 'id')?.type
    if (definition?.ofType !== 'named' || definition.name !== 'ID' || !definition.nonNull) {
      return { errors: [{ message: 'The SMS archive resolver requires an ID! variable.' }] }
    }
    return { data: { [field]: true } }
  })
  const pinia = createPinia()
  const sms = useSmsStore(pinia)
  try {
    expect(await sms[action]([id])).toBe(true)
    expect(showActionError).not.toHaveBeenCalled()
  } finally { sms.$dispose() }
})
