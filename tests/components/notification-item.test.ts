import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it } from 'vitest'
import { defineComponent, h } from 'vue'
import NotificationItem from '@/components/NotificationItem.vue'
import type { INotification } from '@/lib/interfaces'

const ButtonStub = defineComponent({
  name: 'ActionStub',
  emits: ['click'],
  setup(_, { emit, slots }) {
    return () => h('button', { onClick: (e: Event) => emit('click', e) }, slots.default?.())
  },
})

const EmojiTextFieldStub = defineComponent({
  name: 'EmojiTextField',
  props: { modelValue: { type: String, default: '' } },
  setup: (props) => () => h('textarea', { value: props.modelValue }),
})

const DropdownStub = defineComponent({
  name: 'VDropdownStub',
  setup(_, { slots }) {
    return () => h('span', [slots.trigger?.(), slots.default?.()])
  },
})

function makeItem(overrides: Partial<INotification> = {}): INotification {
  return {
    id: 'ntf-1',
    onlyOnce: false,
    isClearable: true,
    appId: 'com.example.app',
    appName: 'Example',
    postedAt: new Date().toISOString(),
    silent: false,
    title: 'Backup done',
    body: 'All files synced',
    icon: 'blob:icon',
    actions: [],
    replyActions: [],
    ...overrides,
  }
}

function mountItem(item: INotification, extraProps: Record<string, unknown> = {}) {
  return mount(NotificationItem, {
    props: { item, replying: false, sending: false, ...extraProps },
    global: {
      mocks: { $t: (key: string) => key },
      directives: { tooltip: () => {} },
      stubs: {
        VOutlinedButton: ButtonStub,
        VFilledButton: ButtonStub,
        EmojiTextField: EmojiTextFieldStub,
        VDropdown: DropdownStub,
      },
    },
  })
}

const wrappers: Array<ReturnType<typeof mount>> = []
afterEach(() => {
  while (wrappers.length) wrappers.pop()!.unmount()
})

describe('NotificationItem', () => {
  it('renders the v2 row structure', () => {
    const wrapper = mountItem(makeItem())
    const root = wrapper.find('article.notification-item')
    expect(root.find('.name').text()).toBe('Example')
    expect(root.find('time').text()).not.toBe('')
    expect(root.find('.ntf-title').text()).toBe('Backup done')
    expect(root.find('.ntf-body').text()).toBe('All files synced')
    expect(root.find('.del').exists()).toBe(true)
    expect(root.find('.dot-unread').exists()).toBe(false)
  })

  it('hides the delete button when not deletable', () => {
    const wrapper = mountItem(makeItem(), { deletable: false })
    expect(wrapper.find('.del').exists()).toBe(false)
  })

  it('exposes the raw notification via the icon view-raw dropdown', () => {
    const wrapper = mountItem(makeItem())
    expect(wrapper.find('.app-ico').exists()).toBe(true)
    expect(wrapper.find('pre.view-raw').text()).toContain('com.example.app')
  })

  it('emits delete from the row delete button', async () => {
    const wrapper = mountItem(makeItem())
    await wrapper.find('.del').trigger('click')
    expect(wrapper.emitted('delete')).toHaveLength(1)
  })

  it('emits reply with the action index', async () => {
    const wrapper = mountItem(makeItem({ replyActions: ['Reply', 'Mark as read'] }))
    const buttons = wrapper.findAll('.reply-actions button')
    expect(buttons.length).toBe(2)
    await buttons[1].trigger('click')
    expect(wrapper.emitted('reply')).toEqual([[1]])
  })

  it('shows the reply box when replying', () => {
    const wrapper = mountItem(makeItem(), { replying: true })
    expect(wrapper.find('.reply-box').exists()).toBe(true)
    expect(wrapper.find('.reply-actions').exists()).toBe(false)
  })
})
