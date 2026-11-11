import { describe, it, expect, vi } from 'vitest'
import { effectScope } from 'vue'
import { useEvents } from '../../src/views/settings/events/useEvents'
import { readEvents, type EventRecord } from '../../src/views/settings/events/api'
vi.mock('../../src/lib/api/gql-client', () => ({ gqlFetch: vi.fn() }))
import { gqlFetch } from '../../src/lib/api/gql-client'
const entry = (id: string): EventRecord => ({ id, type: 'LOGIN', message: 'Fixture', clientId: 'client', createdAt: '2026-01-01T00:00:00Z' })

describe('settings events', () => {
  it('reads the canonical operation and preserves quoted searches', async () => {
    vi.mocked(gqlFetch).mockResolvedValue({ data: { events: [entry('one')] } })
    expect(await readEvents('a"b', 50, 50)).toEqual([entry('one')])
    expect(vi.mocked(gqlFetch).mock.calls[0][0]).toContain('events(query:')
    expect(vi.mocked(gqlFetch).mock.calls[0][1]).toEqual({ query: 'text:"a\\"b"', offset: 50, limit: 50 })
  })
  it('paginates, resets search and surfaces errors', async () => {
    const read = vi.fn().mockResolvedValueOnce(Array.from({ length: 50 }, (_, i) => entry(String(i)))).mockResolvedValueOnce([entry('last')]).mockRejectedValueOnce(new Error('unavailable'))
    const scope = effectScope()
    const state = scope.run(() => useEvents(read))!
    await state.load(true)
    expect(state.hasMore.value).toBe(true)
    await state.load()
    expect(read).toHaveBeenLastCalledWith('', 50, 50)
    expect(state.entries.value).toHaveLength(51)
    expect(state.hasMore.value).toBe(false)
    state.search.value = 'other'
    await state.load(true)
    expect(state.entries.value).toEqual([])
    expect(state.error.value).toBe('unavailable')
    scope.stop()
  })
  it('ignores an older search result after a newer search completes', async () => {
    let finish!: (rows: EventRecord[]) => void
    const read = vi.fn().mockImplementationOnce(() => new Promise<EventRecord[]>(resolve => { finish = resolve })).mockResolvedValueOnce([entry('new')])
    const scope = effectScope()
    const state = scope.run(() => useEvents(read))!
    const old = state.load(true)
    state.search.value = 'new'
    await state.load(true)
    finish([entry('old')]); await old
    expect(state.entries.value.map(row => row.id)).toEqual(['new'])
    scope.stop()
  })
})

it('does not duplicate rows or repeat an offset when new events shift pages', async () => {
  const read = vi.fn().mockResolvedValueOnce(Array.from({ length: 50 }, (_, i) => entry(String(i))))
    .mockResolvedValueOnce(Array.from({ length: 50 }, (_, i) => entry(String(i + 49)))).mockResolvedValueOnce([])
  const scope = effectScope()
  const state = scope.run(() => useEvents(read))!
  await state.load(true); await state.load()
  expect(state.entries.value).toHaveLength(99)
  await state.load()
  expect(read).toHaveBeenLastCalledWith('', 100, 50)
  scope.stop()
})
