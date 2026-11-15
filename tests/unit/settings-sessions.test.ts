import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope } from 'vue'
import { gqlFetch } from '@/lib/api/gql-client'
import { readSessions, renameSession, revokeSession, createApiToken, createApiTokenGQL, sessionsGQL, type SessionRecord } from '@/views/settings/sessions/api'
import { useSessions } from '@/views/settings/sessions/useSessions'

const fixture = vi.hoisted(() => ({ context: 'local' }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn(), getGraphqlContextKey: () => fixture.context }))

const row = (clientId: string, name = 'Fixture session'): SessionRecord => ({
  clientId, clientName: name, type: 'LOGIN', clientIp: '192.0.2.24',
  osName: 'Fixture OS', osVersion: '1', appName: 'Fixture app', appVersion: '1',
  createdAt: '2026-01-01T00:00:00Z', lastSeenAt: undefined,
})
const scopes: ReturnType<typeof effectScope>[] = []
function setup() {
  const scope = effectScope()
  scopes.push(scope)
  return scope.run(useSessions)!
}
beforeEach(() => { vi.resetAllMocks(); fixture.context = 'local' })
afterEach(() => { scopes.splice(0).forEach(scope => scope.stop()) })

describe('sessions API', () => {
  it('reads session metadata through one query', async () => {
    vi.mocked(gqlFetch).mockResolvedValue({ data: { sessions: [row('fixture')] } })
    expect(await readSessions()).toEqual([row('fixture')])
    expect(gqlFetch).toHaveBeenCalledWith(sessionsGQL, {}, { fresh: true })
  })
  it('surfaces GraphQL errors for reads and both mutations', async () => {
    vi.mocked(gqlFetch).mockResolvedValue({ data: null, errors: [{ message: 'no_permission' }] })
    await expect(readSessions()).rejects.toThrow('no_permission')
    await expect(renameSession('fixture', '')).rejects.toThrow('no_permission')
    await expect(revokeSession('fixture')).rejects.toThrow('no_permission')
    await expect(createApiToken('Fixture')).rejects.toThrow('no_permission')
  })
})

describe('session management', () => {
  it('renames with an empty name and refreshes authoritative state after each successful action', async () => {
    const state = setup()
    vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { sessions: [row('fixture')] } })
    await state.load()
    vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { renameSession: true } }).mockResolvedValueOnce({ data: { sessions: [row('fixture', '')] } })
    expect(await state.rename('fixture', '  ')).toBe(true)
    expect(gqlFetch).toHaveBeenNthCalledWith(2, expect.any(String), { clientId: 'fixture', name: '' })
    expect(state.entries.value[0].clientName).toBe('')
    vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { revokeSession: true } }).mockResolvedValueOnce({ data: { sessions: [] } })
    expect(await state.revoke('fixture')).toBe(true)
    expect(state.entries.value).toEqual([])
  })
  it('creates through the selected target main schema and returns the new secret for copying', async () => {
    const state = setup()
    const created = { clientId: 'fixture-token', token: 'synthetic-token' }
    vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { createApiToken: created } }).mockResolvedValueOnce({ data: { sessions: [{ ...row('fixture-token'), type: 'API_TOKEN' }] } })
    expect(await state.create(' Fixture token ')).toEqual(created)
    expect(gqlFetch).toHaveBeenNthCalledWith(1, createApiTokenGQL, { name: 'Fixture token' })
    expect(state.entries.value[0].type).toBe('API_TOKEN')
    expect(state.busy.value).toBe(false)
  })
  it('reports creation failure and permits a later retry', async () => {
    const state = setup()
    vi.mocked(gqlFetch).mockResolvedValueOnce({ data: null, errors: [{ message: 'no_permission' }] })
    expect(await state.create('Fixture token')).toBe(false)
    expect(state.error.value).toBe('no_permission')
    expect(state.busy.value).toBe(false)
    vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { createApiToken: { clientId: 'fixture-token', token: 'synthetic-token' } } }).mockResolvedValueOnce({ data: { sessions: [] } })
    expect(await state.create('Fixture token')).toEqual({ clientId: 'fixture-token', token: 'synthetic-token' })
  })
  it('preserves rows and reports rejected operations without pretending they succeeded', async () => {
    const state = setup()
    vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { sessions: [row('fixture')] } })
    await state.load()
    vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { renameSession: false } })
    expect(await state.rename('fixture', 'New name')).toBe(false)
    expect(state.entries.value).toEqual([row('fixture')])
    expect(state.error.value).toBe('session_management.operation_failed')
    vi.mocked(gqlFetch).mockRejectedValueOnce(new Error('disconnected'))
    expect(await state.revoke('fixture')).toBe(false)
    expect(state.error.value).toBe('disconnected')
    expect(state.busy.value).toBe(false)
  })
  it('discards delayed list and mutation results when the selected target changes', async () => {
    const state = setup()
    let resolveRead!: (value: any) => void
    vi.mocked(gqlFetch).mockImplementationOnce(() => new Promise(resolve => { resolveRead = resolve }))
    const pendingRead = state.load()
    fixture.context = 'peer'
    state.reset()
    vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { sessions: [row('new-target')] } })
    await state.load()
    resolveRead({ data: { sessions: [row('old-target')] } })
    await pendingRead
    expect(state.entries.value.map(entry => entry.clientId)).toEqual(['new-target'])
    let resolveChange!: (value: any) => void
    vi.mocked(gqlFetch).mockImplementationOnce(() => new Promise(resolve => { resolveChange = resolve }))
    const pendingChange = state.rename('new-target', 'New')
    fixture.context = 'third'
    state.reset()
    const count = vi.mocked(gqlFetch).mock.calls.length
    resolveChange({ data: { renameSession: true } })
    expect(await pendingChange).toBe(false)
    expect(gqlFetch).toHaveBeenCalledTimes(count)
    expect(state.entries.value).toEqual([])
    expect(state.error.value).toBe('')
  })
})
