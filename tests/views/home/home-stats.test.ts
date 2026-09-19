import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

const harness = vi.hoisted(() => ({ initQueryCalls: [] as { document: () => string }[] }))

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn(), toastWithAction: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { on: vi.fn(), off: vi.fn(), emit: vi.fn() } }))
vi.mock('@/hooks/contact-picker', () => ({ useContactPicker: () => ({}) }))
vi.mock('@/lib/api/mutation', async () => {
  const { ref } = await vi.importActual<typeof import('vue')>('vue')
  return {
    callGQL: 'call',
    pauseMediaScanGQL: 'pause',
    resumeMediaScanGQL: 'resume',
    stopMediaScanGQL: 'stop',
    rebuildMediaIndexGQL: 'rebuild',
    initMutation: () => ({ mutate: vi.fn(), loading: ref(false) }),
  }
})
vi.mock('@/lib/api/query', async (importOriginal) => {
  const { ref } = await vi.importActual<typeof import('vue')>('vue')
  const actual = await importOriginal<typeof import('@/lib/api/query')>()
  return {
    ...actual,
    simsGQL: 'sims',
    scanProgressGQL: 'scan-progress',
    initQuery: (params: { document: () => string }) => {
      harness.initQueryCalls.push(params)
      return { loading: ref(false), result: ref(), refetch: vi.fn() }
    },
  }
})

import { useHomeData } from '@/views/home/home'
import { useTempStore } from '@/stores/temp'
import { DeviceFeature } from '@/lib/data'

const PHONE_FEATURES = Object.values(DeviceFeature)
const NAS_FEATURES = ['MEDIA_TRASH', 'DOC_PREVIEW', 'MEDIA_SCAN']

function setFeatures(features: string[]) {
  const tempStore = useTempStore()
  tempStore.app = { ...tempStore.app, features }
}

beforeEach(() => {
  setActivePinia(createPinia())
  harness.initQueryCalls.length = 0
})

describe('useHomeData homeStats document', () => {
  it('requests docCount for a NAS alongside the media counts', () => {
    setFeatures(NAS_FEATURES)
    useHomeData()
    const query = harness.initQueryCalls.at(-1)!.document()
    expect(query).toContain('docCount(query: "")')
    expect(query).toContain('audioCount(query: $mediaQuery)')
    expect(query).toContain('imageCount(query: $mediaQuery)')
    expect(query).toContain('videoCount(query: $mediaQuery)')
    expect(query).not.toContain('smsCount')
    expect(query).toContain('mounts')
  })

  it('keeps the phone set unchanged with docs present', () => {
    setFeatures(PHONE_FEATURES)
    useHomeData()
    const query = harness.initQueryCalls.at(-1)!.document()
    expect(query).toContain('docCount(query: "")')
    expect(query).toContain('smsCount')
    expect(query).toContain('contactCount')
  })

  it('stays a mounts-only probe before the feature list is known', () => {
    useHomeData()
    const query = harness.initQueryCalls.at(-1)!.document()
    expect(query).not.toContain('Count(')
    expect(query).toContain('mounts')
  })
})
