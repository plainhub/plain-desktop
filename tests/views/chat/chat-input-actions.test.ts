import { mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h } from 'vue'
import ChatInput from '@/views/chat/ChatInput.vue'

vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

// Mirrors the production shape: a real textarea wired to the events ChatInput
// listens on, plus the footer slot so the toolbar buttons render.
const EmojiTextFieldStub = defineComponent({
  name: 'EmojiTextField',
  props: { modelValue: { type: String, default: '' } },
  emits: ['update:modelValue', 'paste', 'keydown', 'compositionstart', 'compositionend'],
  setup(props, { emit, slots }) {
    return () =>
      h('div', [
        h('textarea', {
          class: 'textarea',
          value: props.modelValue,
          onInput: (e: Event) => emit('update:modelValue', (e.target as HTMLTextAreaElement).value),
          onKeydown: (e: KeyboardEvent) => emit('keydown', e),
          onPaste: (e: ClipboardEvent) => emit('paste', e),
          onCompositionstart: (e: CompositionEvent) => emit('compositionstart', e),
          onCompositionend: (e: CompositionEvent) => emit('compositionend', e),
        }),
        slots.footer?.(),
      ])
  },
})

const VIconButtonStub = defineComponent({
  name: 'VIconButton',
  inheritAttrs: false,
  props: { disabled: { type: Boolean, default: false } },
  emits: ['click'],
  setup(props, { attrs, emit, slots }) {
    return () => h('button', { ...attrs, disabled: props.disabled, onClick: () => emit('click') }, slots.default?.())
  },
})

function mountInput(modelValue = '') {
  return mount(ChatInput, {
    props: { modelValue, createLoading: false },
    global: {
      mocks: { $t: (key: string) => key },
      directives: { tooltip: () => {} },
      stubs: { EmojiTextField: EmojiTextFieldStub, VIconButton: VIconButtonStub },
    },
  })
}

const wrappers: Array<ReturnType<typeof mount>> = []
afterEach(() => {
  while (wrappers.length) wrappers.pop()!.unmount()
  vi.restoreAllMocks()
})

function toolbarButtons(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll('.input-toolbar button')
}

describe('ChatInput actions', () => {
  it('shows no capture button on web and keeps image, folder, send order', () => {
    const wrapper = mountInput()
    wrappers.push(wrapper)
    expect(wrapper.find('[data-testid="screen-capture-button"]').exists()).toBe(false)
    const buttons = toolbarButtons(wrapper)
    expect(buttons).toHaveLength(3)
    expect(buttons[0].element.getAttribute('data-testid')).toBeNull()
  })

  it('opens the media picker from the image button and the file picker from the folder button', async () => {
    const wrapper = mountInput()
    wrappers.push(wrapper)
    const clicked: HTMLInputElement[] = []
    vi.spyOn(HTMLInputElement.prototype, 'click').mockImplementation(function (this: HTMLInputElement) {
      clicked.push(this)
    })
    const [imageButton, folderButton] = toolbarButtons(wrapper)
    await imageButton.trigger('click')
    await folderButton.trigger('click')
    expect(clicked).toHaveLength(2)
    expect(clicked[0].accept).toBe('image/*, video/*')
    expect(clicked[1].accept).toBe('')
    expect(clicked[1].multiple).toBe(true)
  })

  it('sends on the send button click and respects disabled state', async () => {
    const wrapper = mount(ChatInput, {
      props: { modelValue: '', createLoading: true },
      global: {
        mocks: { $t: (key: string) => key },
        directives: { tooltip: () => {} },
        stubs: { EmojiTextField: EmojiTextFieldStub, VIconButton: VIconButtonStub },
      },
    })
    wrappers.push(wrapper)
    const send = toolbarButtons(wrapper)[2]
    expect(send.element.disabled).toBe(true)
    await send.trigger('click')
    expect(wrapper.emitted('send-message')).toBeUndefined()

    await wrapper.setProps({ createLoading: false })
    await toolbarButtons(wrapper)[2].trigger('click')
    expect(wrapper.emitted('send-message')).toHaveLength(1)
  })

  it('enter sends, shift+enter inserts a newline, typing emits updates', async () => {
    const wrapper = mountInput()
    wrappers.push(wrapper)
    const textarea = wrapper.find('textarea')
    await textarea.setValue('hello')
    expect(wrapper.emitted('update:modelValue')!.at(-1)).toEqual(['hello'])
    await textarea.trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('send-message')).toHaveLength(1)
    await textarea.trigger('keydown', { key: 'Enter', shiftKey: true })
    expect(wrapper.emitted('update:modelValue')!.at(-1)).toEqual(['\n'])
    expect(wrapper.emitted('send-message')).toHaveLength(1)
  })

  it('composition input never triggers send', async () => {
    const wrapper = mountInput()
    wrappers.push(wrapper)
    const textarea = wrapper.find('textarea')
    await textarea.trigger('compositionstart')
    await textarea.trigger('keydown', { key: 'Enter', isComposing: true, keyCode: 229 })
    expect(wrapper.emitted('send-message')).toBeUndefined()
    await textarea.trigger('compositionend')
    await textarea.trigger('keydown', { key: 'Enter' })
    expect(wrapper.emitted('send-message')).toHaveLength(1)
  })

  it('file picker change emits send-files and image picker change emits send-images', async () => {
    const wrapper = mountInput()
    wrappers.push(wrapper)
    const inputs = wrapper.findAll('input[type="file"]')
    const txt = new File(['a'], 'doc.txt', { type: 'text/plain' })
    const png = new File(['b'], 'pic.png', { type: 'image/png' })
    Object.defineProperty(inputs[0].element, 'files', { value: [txt] })
    await inputs[0].trigger('change')
    expect(wrapper.emitted('send-files')![0]).toEqual([[txt]])
    Object.defineProperty(inputs[1].element, 'files', { value: [png] })
    await inputs[1].trigger('change')
    expect(wrapper.emitted('send-images')![0]).toEqual([[png]])
  })

  it('dropping files splits media and documents', async () => {
    const wrapper = mountInput()
    wrappers.push(wrapper)
    const png = new File(['x'], 'a.png', { type: 'image/png' })
    const txt = new File(['y'], 'b.txt', { type: 'text/plain' })
    const event = new Event('drop') as DragEvent
    Object.defineProperty(event, 'dataTransfer', { value: { files: [png, txt] } })
    await wrapper.find('.textarea-wrapper').element.dispatchEvent(event)
    await wrapper.vm.$nextTick()
    expect(wrapper.emitted('send-images')![0]).toEqual([[png]])
    expect(wrapper.emitted('send-files')![0]).toEqual([[txt]])
  })

  it('pasting files emits media split and blocks the default text insertion', async () => {
    const wrapper = mountInput()
    wrappers.push(wrapper)
    const png = new File(['x'], 'a.png', { type: 'image/png' })
    const event = new Event('paste') as ClipboardEvent
    Object.defineProperty(event, 'clipboardData', { value: { items: [{ kind: 'file', getAsFile: () => png }] } })
    const preventDefault = vi.spyOn(event, 'preventDefault')
    await wrapper.find('textarea').element.dispatchEvent(event)
    await wrapper.vm.$nextTick()
    expect(wrapper.emitted('send-images')![0]).toEqual([[png]])
    expect(preventDefault).toHaveBeenCalled()
  })
})
