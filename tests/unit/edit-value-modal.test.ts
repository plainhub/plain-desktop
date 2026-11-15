import { afterEach, expect, it, vi } from 'vitest'
import { h, ref } from 'vue'
import EditValueModal from '@/components/EditValueModal.vue'
import { popModal } from '@/components/modal'
import { clientTemplate, renderView, nodes, click, fire, flush, ModalShell } from '../helpers/vue-node-renderer'

vi.mock('@/components/modal', () => ({ popModal: vi.fn().mockResolvedValue(undefined) }))
clientTemplate(EditValueModal, 'src/components/EditValueModal.vue')
const mounted: Array<() => void> = []
afterEach(() => { mounted.splice(0).forEach(unmount => unmount()); vi.unstubAllGlobals(); vi.clearAllMocks() })
function mount(value: string, allowEmpty = false) {
  vi.stubGlobal('requestAnimationFrame', () => 0)
  const loading = ref(false)
  let done!: () => void
  const mutate = vi.fn()
  const view = renderView(EditValueModal, {
    title: 'Fixture name', value, allowEmpty,
    mutation: () => ({ loading, mutate, onDone: (callback: () => void) => { done = callback } }),
    getVariables: (name: string) => ({ name: name.trim() }),
  }, {
    VModal: ModalShell,
    VTextField: {
      props: { modelValue: { type: String, default: '' } }, emits: ['update:modelValue', 'keyup.enter'],
      setup(props: any, { emit }: any) { return () => h('input', { value: props.modelValue, onInput: (name: string) => emit('update:modelValue', name), onKeyup: () => emit('keyup.enter') }) },
    },
    VFilledButton: { setup(_: unknown, { slots }: any) { return () => h('button', slots.default?.()) } },
    VOutlinedButton: { setup(_: unknown, { slots }: any) { return () => h('button', slots.default?.()) } },
  })
  mounted.push(view.unmount)
  return { ...view, loading, mutate, done: () => done() }
}

it('prefills the input and allows a session name to be cleared', async () => {
  const view = mount('Fixture OS 1', true)
  await flush()
  const input = nodes(view.root).find(node => node.tag === 'input')!
  expect(input.props.value).toBe('Fixture OS 1')
  input.props.onInput('')
  await flush()
  click(nodes(view.root).find(node => node.tag === 'button' && node.props.value === 'save')!)
  expect(view.mutate).toHaveBeenCalledWith({ name: '' })
  view.done()
  expect(popModal).toHaveBeenCalledOnce()
})

it('preserves required-name validation for other callers and blocks repeated Enter while saving', async () => {
  const view = mount('')
  await flush()
  const input = nodes(view.root).find(node => node.tag === 'input')!
  fire(input, 'onKeyup', { key: 'Enter' })
  expect(view.mutate).not.toHaveBeenCalled()
  input.props.onInput('Fixture')
  await flush()
  view.loading.value = true
  await flush()
  fire(input, 'onKeyup', { key: 'Enter' })
  expect(view.mutate).not.toHaveBeenCalled()
  view.loading.value = false
  await flush()
  fire(input, 'onKeyup', { key: 'Enter' })
  expect(view.mutate).toHaveBeenCalledWith({ name: 'Fixture' })
})
