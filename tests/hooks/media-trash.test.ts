import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@/lib/api/mutation', () => ({
  trashMediaItemsGQL: 'trash-items',
  restoreMediaItemsGQL: 'restore-items',
  initMutation: () => ({ mutate: vi.fn(), onDone: vi.fn() }),
}))

import { useFileTrashState } from '@/hooks/media-trash'
import { useTempStore } from '@/stores/temp'
import { DataType } from '@/lib/data'
import type { ISource } from '@/components/lightbox/types'

function source(overrides: Partial<ISource> = {}): ISource {
  return {
    src: 'blob:src',
    path: '/photos/pic.png',
    name: 'pic.png',
    size: 1,
    duration: 0,
    type: DataType.IMAGE,
    ...overrides,
  }
}

function setFeatures(features: string[]) {
  const tempStore = useTempStore()
  tempStore.app = { ...tempStore.app, capabilities: features }
}

beforeEach(() => {
  setActivePinia(createPinia())
})

describe('useFileTrashState canTrash', () => {
  it('allows trashing when the server declares MEDIA_TRASH', () => {
    setFeatures(['MEDIA_TRASH'])
    const { canTrash } = useFileTrashState(() => source())
    expect(canTrash.value).toBe(true)
  })

  it('hides trash when MEDIA_TRASH is not declared', () => {
    setFeatures([])
    expect(useFileTrashState(() => source()).canTrash.value).toBe(false)
  })

  it('only applies to media types', () => {
    setFeatures(['MEDIA_TRASH'])
    expect(useFileTrashState(() => source({ type: DataType.DOC })).canTrash.value).toBe(false)
    expect(useFileTrashState(() => source({ type: DataType.VIDEO })).canTrash.value).toBe(true)
    expect(useFileTrashState(() => source({ type: DataType.AUDIO })).canTrash.value).toBe(true)
  })

  it('reacts to feature changes', () => {
    setFeatures([])
    const state = useFileTrashState(() => source())
    expect(state.canTrash.value).toBe(false)
    setFeatures(['MEDIA_TRASH'])
    expect(state.canTrash.value).toBe(true)
  })
})

describe('useFileTrashState isTrashed', () => {
  it('detects phone MediaStore trash naming (.trashed- prefix)', () => {
    const { isTrashed } = useFileTrashState(() => source({ path: '/storage/emulated/0/Pictures/.trashed-1234-pic.png' }))
    expect(isTrashed.value).toBe(true)
  })

  it('detects NAS trash tree (/.nas-trash/)', () => {
    const { isTrashed } = useFileTrashState(() => source({ path: '/data/.nas-trash/2026/09/f_abc123_pic.png' }))
    expect(isTrashed.value).toBe(true)
  })

  it('does not flag normal paths', () => {
    const { isTrashed } = useFileTrashState(() => source())
    expect(isTrashed.value).toBe(false)
  })
})
