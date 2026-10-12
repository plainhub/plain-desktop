import { afterEach, describe, expect, it, vi } from 'vitest'
import { nativeGraphql } from '@/lib/api/native-client'

const invoke = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/core', () => ({ invoke }))

afterEach(() => {
  vi.useRealTimers()
  invoke.mockReset()
})

describe('nativeGraphql', () => {
  it('sends the request envelope to the shared core and returns its response', async () => {
    const result = { data: { app: { name: 'Plain' } } }
    invoke.mockResolvedValue(result)

    await expect(nativeGraphql({ query: 'query { app { name } }' }, 30000)).resolves.toBe(result)
    expect(invoke).toHaveBeenCalledWith('native_graphql', {
      body: { query: 'query { app { name } }' },
    })
  })

  it('passes variables through unchanged', async () => {
    invoke.mockResolvedValue({ data: {} })

    await nativeGraphql({ query: 'query X($id: ID!) { note(id: $id) { id } }', variables: { id: '7' } }, 30000)

    expect(invoke).toHaveBeenCalledWith('native_graphql', {
      body: {
        query: 'query X($id: ID!) { note(id: $id) { id } }',
        variables: { id: '7' },
      },
    })
  })

  it('surfaces GraphQL errors in the response instead of throwing', async () => {
    const response = { data: null, errors: [{ message: 'not authorized' }] }
    invoke.mockResolvedValue(response)

    await expect(nativeGraphql({ query: 'query { app }' }, 30000)).resolves.toBe(response)
  })

  it('turns a core rejection into the core message', async () => {
    invoke.mockRejectedValue('Rust core stopped')

    await expect(nativeGraphql({ query: 'query { app }' }, 30000)).rejects.toThrow('Rust core stopped')
  })

  it('falls back to native_error when the rejection carries no message', async () => {
    invoke.mockRejectedValue({})

    await expect(nativeGraphql({ query: 'query { app }' }, 30000)).rejects.toThrow('native_error')
  })

  it('gives up after the timeout instead of waiting on the core forever', async () => {
    vi.useFakeTimers()
    invoke.mockReturnValue(new Promise(() => {}))

    const pending = nativeGraphql({ query: 'query { app }' }, 30000)
    const assertion = expect(pending).rejects.toThrow('connection_timeout')
    await vi.advanceTimersByTimeAsync(30000)
    await assertion
  })
})
