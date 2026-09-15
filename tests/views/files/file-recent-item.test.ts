import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { createPinia } from 'pinia'
import { defineComponent, h } from 'vue'
import FileRecentItem from '@/views/files/FileRecentItem.vue'
import type { IFile } from '@/lib/file'

const file = (name: string): IFile =>
  ({ id: name, name, path: `/recent/${name}`, isDir: false, updatedAt: '2026-09-16T00:00:00Z', createdAt: '2026-09-16T00:00:00Z', extension: '', size: 1 }) as IFile

const noop = () => {}

function mountItem(name: string, isPhone = false) {
  return mount(FileRecentItem, {
    props: {
      item: file(name),
      index: 0,
      selectedIds: [],
      shiftEffectingIds: [],
      shouldSelect: false,
      isPhone,
      imageErrorIds: [],
      extensionImageErrorIds: [],
      handleItemClick: noop,
      handleMouseOver: noop,
      toggleSelect: noop,
      onImageError: noop,
      onExtensionImageError: noop,
      downloadFile: noop,
      clickItem: noop,
    },
    global: {
      plugins: [createPinia()],
      mocks: { $t: (key: string) => key },
      directives: { tooltip: () => {} },
      stubs: {
        VCheckbox: defineComponent({ name: 'VCheckboxStub', setup: () => () => h('span', { class: 'checkbox-stub' }) }),
        FieldId: defineComponent({ name: 'FieldIdStub', setup: () => () => h('span', { class: 'field-id-stub' }) }),
        FileThumb: defineComponent({ name: 'FileThumbStub', setup: () => () => h('span', { class: 'thumb-stub' }) }),
        VIconButton: defineComponent({ name: 'VIconButtonStub', setup: (_, { slots }) => () => h('button', slots.default?.()) }),
        VDropdown: defineComponent({ name: 'VDropdownStub', setup: (_, { slots }) => () => h('span', [slots.trigger?.(), slots.default?.()]) }),
        ListItemPhone: defineComponent({ name: 'ListItemPhoneStub', setup: (_, { slots }) => () => h('div', { class: 'list-item-phone' }, [slots.image?.(), slots.title?.(), slots.subtitle?.(), slots.actions?.()]) }),
      },
    },
  })
}

describe('FileRecentItem', () => {
  it.each([
    ['desktop', false],
    ['phone', true],
  ])('shows online preview marker for a text file (%s)', (label, isPhone) => {
    const wrapper = mountItem('AndroidManifest.xml', isPhone)
    expect(wrapper.find('.online-preview-icon').exists()).toBe(true)
  })

  it.each([
    ['desktop', false],
    ['phone', true],
  ])('hides online preview marker for non-previewable files (%s)', (label, isPhone) => {
    const wrapper = mountItem('video.mp4', isPhone)
    expect(wrapper.find('.online-preview-icon').exists()).toBe(false)
  })
})
