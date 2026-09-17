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
import { DeviceType } from '@/lib/status'
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

function setDevice(deviceType: DeviceType, osVersion: number) {
  const tempStore = useTempStore()
  tempStore.app = { ...tempStore.app, deviceType, osVersion }
}

beforeEach(() => {
  setActivePinia(createPinia())
})

describe('useFileTrashState canTrash', () => {
  it('allows trashing on a NAS even though osVersion is not an Android SDK level', () => {
    setDevice(DeviceType.NAS, 0)
    const { canTrash } = useFileTrashState(() => source())
    expect(canTrash.value).toBe(true)
  })

  it.each([DeviceType.PHONE, DeviceType.TABLET])(
    'still requires Android R+ on %s (no NAS bypass leaked to phones)',
    (deviceType) => {
      setDevice(deviceType, 29)
      expect(useFileTrashState(() => source()).canTrash.value).toBe(false)
      setDevice(deviceType, 30)
      expect(useFileTrashState(() => source()).canTrash.value).toBe(true)
    },
  )

  it('only applies to media types', () => {
    setDevice(DeviceType.NAS, 0)
    expect(useFileTrashState(() => source({ type: DataType.DOC })).canTrash.value).toBe(false)
    expect(useFileTrashState(() => source({ type: DataType.VIDEO })).canTrash.value).toBe(true)
    expect(useFileTrashState(() => source({ type: DataType.AUDIO })).canTrash.value).toBe(true)
  })

  it('reacts to device changes', () => {
    setDevice(DeviceType.PHONE, 29)
    const state = useFileTrashState(() => source())
    expect(state.canTrash.value).toBe(false)
    setDevice(DeviceType.NAS, 29)
    expect(state.canTrash.value).toBe(true)
  })
})

describe('useFileTrashState isTrashed', () => {
  beforeEach(() => {
    setDevice(DeviceType.NAS, 0)
  })

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
