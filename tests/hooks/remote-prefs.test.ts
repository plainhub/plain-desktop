import { afterEach, describe, expect, it, vi } from 'vitest'
import { effectScope, reactive } from 'vue'
import { useRemotePrefs } from '@/hooks/remote-prefs'
import { userPrefsGQL } from '@/lib/api/query'
import { setUserPrefGQL } from '@/lib/api/mutation'
import { gqlFetch } from '@/lib/api/gql-client'
import type { useMainStore } from '@/stores/main'

vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: vi.fn(), GqlError: class extends Error {} }))

afterEach(() => vi.clearAllMocks())

describe('remote preferences', () => {
  it('uses server values and imports only preferences missing on the server', async () => {
    const fetch = vi.mocked(gqlFetch)
    fetch.mockImplementation(async (document) => document === userPrefsGQL
      ? { data: { userPrefs: { excludedDirs: ['/remote/'] } } }
      : { data: { setUserPref: true } })

    const store = reactive({
      excludedDirs: ['/local/'],
      railFeatures: ['files'],
      homeFeatures: ['audios'],
    }) as unknown as ReturnType<typeof useMainStore>
    const scope = effectScope()
    const enabled = scope.run(() => useRemotePrefs(store, {
      excludedDirs: ['/local/'], railFeatures: ['files'], homeFeatures: ['audios'],
    }))!

    await vi.waitFor(() => expect(enabled.value).toBe(true))
    expect(store.excludedDirs).toEqual(['/remote/'])
    expect(fetch).toHaveBeenCalledWith(setUserPrefGQL, { key: 'railFeatures', value: ['files'] }, { dedupe: false })
    expect(fetch).toHaveBeenCalledWith(setUserPrefGQL, { key: 'homeFeatures', value: ['audios'] }, { dedupe: false })
    expect(fetch).not.toHaveBeenCalledWith(setUserPrefGQL, expect.objectContaining({ key: 'excludedDirs' }), expect.anything())
    store.excludedDirs = ['/changed/']
    await vi.waitFor(() => expect(fetch).toHaveBeenCalledWith(
      setUserPrefGQL, { key: 'excludedDirs', value: ['/changed/'] }, { dedupe: false },
    ))
    scope.stop()
  })

  it('keeps local storage active if the server has no prefs API', async () => {
    vi.mocked(gqlFetch).mockResolvedValue({ data: undefined, errors: [{ message: 'Unknown field prefs' }] })
    const store = reactive({ excludedDirs: ['/local/'], railFeatures: [], homeFeatures: [] }) as unknown as ReturnType<typeof useMainStore>
    const scope = effectScope()
    const enabled = scope.run(() => useRemotePrefs(store, null))!

    await vi.waitFor(() => expect(gqlFetch).toHaveBeenCalled())
    expect(enabled.value).toBe(false)
    expect(store.excludedDirs).toEqual(['/local/'])
    scope.stop()
  })
})
