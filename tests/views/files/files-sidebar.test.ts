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

import FilesSidebar from '@/views/files/FilesSidebar.vue'
import LeftSidebar from '@/components/LeftSidebar.vue'
import SidebarListItem from '@/components/SidebarListItem.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import VolumeCard from '@/components/storage/VolumeCard.vue'
import { useTempStore } from '@/stores/temp'
import { DeviceType } from '@/lib/status'
import type { IApp } from '@/lib/interfaces'

const VDropdownMenuStub = defineComponent({
  name: 'VDropdownMenu',
  setup: () => () => null,
})

function mountSidebar(app: Partial<IApp>) {
  const pinia = createPinia()
  setActivePinia(pinia)
  const temp = useTempStore()
  temp.app = { ...temp.app, ...app }
  return mount(FilesSidebar, {
    global: {
      plugins: [pinia],
      components: {
        LeftSidebar,
        SidebarListItem,
        VIconButton,
        VolumeCard,
        VDropdownMenu: VDropdownMenuStub,
      },
      mocks: { $t: (key: string) => key },
      directives: { tooltip: {} },
    },
  })
}

const diskManagerButton = '.section-title button'

describe('FilesSidebar disk manager button', () => {
  it('shows the button when app.features declares DISK_MANAGER, regardless of device type', () => {
    const wrapper = mountSidebar({ deviceType: DeviceType.PHONE, features: ['DISK_MANAGER'] })
    const button = wrapper.find(diskManagerButton)
    expect(button.exists()).toBe(true)
  })

  it('hides the button for a NAS that does not declare DISK_MANAGER', () => {
    const wrapper = mountSidebar({ deviceType: DeviceType.NAS, features: ['MEDIA_TRASH', 'MEDIA_SCAN'] })
    expect(wrapper.find(diskManagerButton).exists()).toBe(false)
  })

  it('keeps the button hidden until the app query resolves features', () => {
    const wrapper = mountSidebar({ deviceType: DeviceType.NAS, features: undefined as unknown as string[] })
    expect(wrapper.find(diskManagerButton).exists()).toBe(false)
  })
})
