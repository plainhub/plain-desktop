import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import { defineComponent, h } from 'vue'

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/lib/api/mutation', () => ({ openWebSettingsGQL: 'mutation {}', initMutation: () => ({ mutate: vi.fn(), loading: vi.fn(() => false), onDone: () => {}, onError: () => {} }) }))
vi.mock('@/plugins/tapphone', () => ({ default: vi.fn() }))

import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'

const TextButtonStub = defineComponent({
  name: 'VTextButton',
  setup(_, { slots }) {
    return () => h('button', slots.default?.())
  },
})

const mountIt = (props: Record<string, unknown>) =>
  mount(NoDataPlaceholder, {
    props,
    global: {
      mocks: { $t: (key: string) => key },
      stubs: { VTextButton: TextButtonStub },
    },
  })

describe('NoDataPlaceholder', () => {
  it('absent online prop means online — shows no_permission, never offline', () => {
    const w = mountIt({ loading: false, permissions: [], permission: 'WRITE_EXTERNAL_STORAGE' })
    expect(w.text()).toContain('no_permission')
    expect(w.text()).not.toContain('offline')
    expect(w.find('button').exists()).toBe(true)
  })

  it('online: false explicitly shows offline without a settings button', () => {
    const w = mountIt({ loading: false, online: false, permissions: [], permission: 'WRITE_EXTERNAL_STORAGE' })
    expect(w.text()).toContain('offline')
    expect(w.find('button').exists()).toBe(false)
  })

  it('absent showSettingsLink falls back to auto — hidden for no_data, shown for no_permission', () => {
    const withPermission = mountIt({ loading: false, permissions: ['WRITE_EXTERNAL_STORAGE'], permission: 'WRITE_EXTERNAL_STORAGE' })
    expect(withPermission.text()).toContain('no_data')
    expect(withPermission.find('button').exists()).toBe(false)

    const withoutPermission = mountIt({ loading: false, permissions: [], permission: 'WRITE_EXTERNAL_STORAGE' })
    expect(withoutPermission.find('button').exists()).toBe(true)
  })

  it('showSettingsLink: false suppresses the button even for no_permission', () => {
    const w = mountIt({ loading: false, permissions: [], permission: 'WRITE_EXTERNAL_STORAGE', showSettingsLink: false })
    expect(w.text()).toContain('no_permission')
    expect(w.find('button').exists()).toBe(false)
  })
})
