import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const invoke = vi.hoisted(() => vi.fn())
const httpRequest = vi.hoisted(() => vi.fn())
const target = vi.hoisted(() => ({ native: true }))

vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@/lib/api/http', () => ({ httpRequest }))
vi.mock('@/lib/api/api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/api/api')>('@/lib/api/api')
  return { ...actual, isNativeTarget: () => target.native }
})

const { gqlFetch, GqlError } = await import('@/lib/api/gql-client')

beforeEach(() => {
  target.native = true
})

afterEach(() => {
  invoke.mockReset()
  httpRequest.mockReset()
})

describe('gqlFetch target selection', () => {
  it('sends the local target to the shared core instead of local HTTP', async () => {
    const result = { data: { app: { name: 'Plain' } } }
    invoke.mockResolvedValue(result)

    await expect(gqlFetch('query { app { name } }', undefined, { dedupe: false })).resolves.toBe(result)

    expect(invoke).toHaveBeenCalledWith('native_graphql', {
      body: { query: 'query { app { name } }', variables: undefined },
    })
    expect(httpRequest).not.toHaveBeenCalled()
  })

  it('leaves a bound device on the encrypted HTTP transport', async () => {
    target.native = false
    httpRequest.mockRejectedValue(new Error('not configured in this test'))

    await expect(gqlFetch('query { app { name } }', undefined, { dedupe: false })).rejects.toBeInstanceOf(GqlError)

    expect(invoke).not.toHaveBeenCalled()
    expect(httpRequest).toHaveBeenCalled()
  })

  it('reports a core failure as a GqlError so callers keep their error contract', async () => {
    invoke.mockRejectedValue('Rust core stopped')

    await expect(gqlFetch('query { app { name } }', undefined, { dedupe: false })).rejects.toThrow(new GqlError('Rust core stopped'))
  })

  it('dedupes identical local requests like every other target', async () => {
    const result = { data: { ok: true } }
    invoke.mockResolvedValue(result)

    const [first, second] = await Promise.all([gqlFetch('query Same { ok }', undefined, { fresh: true }), gqlFetch('query Same { ok }')])

    expect(first).toBe(result)
    expect(second).toBe(result)
    expect(invoke).toHaveBeenCalledTimes(1)
  })
})
