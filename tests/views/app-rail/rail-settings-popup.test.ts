import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, h } from 'vue'
import { describe, expect, it, vi } from 'vitest'

vi.mock('vue-i18n', async (importOriginal) => {
  const { ref } = await vi.importActual<typeof import('vue')>('vue')
  return {
    ...(await importOriginal<typeof import('vue-i18n')>()),
    useI18n: () => ({ t: (key: string) => key, locale: ref('en-US') }),
  }
})

vi.mock('@/lib/api/query', async (importOriginal) => {
  const { ref } = await vi.importActual<typeof import('vue')>('vue')
  return {
    ...(await importOriginal<typeof import('@/lib/api/query')>()),
    initQuery: () => ({ loading: ref(false), result: ref(undefined), refetch: () => Promise.resolve() }),
  }
})

import RailSettingsPopup from '@/views/app-rail/RailSettingsPopup.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import { useTempStore } from '@/stores/temp'
import { DeviceType } from '@/lib/status'
import type { IApp } from '@/lib/interfaces'

const VDropdownSlot = defineComponent({
  name: 'VDropdown',
  setup: (_, { slots }) => () => h('div', { class: 'dropdown-slot' }, slots.default?.()),
})

const ThemeChangerStub = defineComponent({
  name: 'ThemeChanger',
  setup: () => () => h('div', { class: 'theme-changer-stub' }),
})

const RouterLinkStub = defineComponent({
  name: 'RouterLink',
  props: { to: { type: [String, Object], required: true } },
  setup: (props, { slots }) => () =>
    h('a', { href: typeof props.to === 'string' ? props.to : '' }, slots.default?.()),
})

function mountPopup(app: Partial<IApp>) {
  const pinia = createPinia()
  setActivePinia(pinia)
  const temp = useTempStore()
  temp.app = { ...temp.app, ...app }
  return mount(RailSettingsPopup, {
    global: {
      plugins: [pinia],
      components: {
        VDropdown: VDropdownSlot,
        VIconButton,
        RouterLink: RouterLinkStub,
        ThemeChanger: ThemeChangerStub,
      },
      mocks: { $t: (key: string) => key },
      directives: { tooltip: {} },
    },
  })
}

const lanShareEntry = 'a[href="/settings/lan-share"]'

describe('RailSettingsPopup LAN share entry', () => {
  it('shows the entry when app.features declares LAN_SHARE, regardless of device type', () => {
    const wrapper = mountPopup({ deviceType: DeviceType.PHONE, features: ['LAN_SHARE'] })
    const entry = wrapper.find(lanShareEntry)
    expect(entry.exists()).toBe(true)
    expect(entry.text()).toContain('lan_share')
  })

  it('hides the entry for a NAS that does not declare LAN_SHARE', () => {
    const wrapper = mountPopup({ deviceType: DeviceType.NAS, features: ['MEDIA_TRASH', 'MEDIA_SCAN'] })
    expect(wrapper.find(lanShareEntry).exists()).toBe(false)
  })

  it('keeps the entry hidden until the app query resolves features', () => {
    const wrapper = mountPopup({ deviceType: DeviceType.NAS, features: undefined as unknown as string[] })
    expect(wrapper.find(lanShareEntry).exists()).toBe(false)
  })
})
