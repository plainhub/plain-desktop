import { afterEach, expect, it, vi } from 'vitest'
import { createSSRApp, h, ref } from 'vue'
import { renderToString } from 'vue/server-renderer'
import EditValueModal from '@/components/EditValueModal.vue'
import { fileNameError } from '@/lib/file'
import { popModal } from '@/components/modal'

vi.mock('@/components/modal', () => ({ popModal: vi.fn() }))
vi.mock('@/lib/api/file', () => ({}))
afterEach(() => { vi.clearAllMocks(); vi.unstubAllGlobals() })

async function modal(value: string, done = (_: string): boolean | void => undefined) {
  let bindings!: { doAction(): void; errors: { name: string }; name: { value: string } }
  let complete!: () => void
  const mutate = vi.fn()
  const component = EditValueModal as any
  const app = createSSRApp({ ...component, setup(props: unknown, context: unknown) {
    bindings = component.setup(props, context)
    return bindings
  } }, {
    title: 'File name', value, validate: fileNameError, done,
    getVariables: (name: string) => ({ name }),
    mutation: () => ({ mutate, loading: ref(false), onDone: (callback: () => void) => { complete = callback } }),
  })
  app.config.globalProperties.$t = (key: string) => key
  app.component('v-modal', { setup: (_: unknown, { slots }: any) => () => h('div', [slots.content?.(), slots.actions?.()]) })
  for (const name of ['v-text-field', 'v-outlined-button', 'v-filled-button']) app.component(name, { render: () => null })
  await renderToString(app)
  bindings.doAction()
  return { bindings, mutate, complete }
}

it.each(['.', '..', '../escaped', 'nested/name', 'nested\\name'])('retains and rejects the filename %s before submitting', async value => {
  const view = await modal(value)
  expect(view.mutate).not.toHaveBeenCalled()
  expect(view.bindings.errors.name).toBe('invalid_file_path')
  expect(view.bindings.name.value).toBe(value)
  expect(popModal).not.toHaveBeenCalled()
})

it('retains the dialog when the operation completion rejects its result', async () => {
  const done = vi.fn().mockReturnValue(false)
  const view = await modal('renamed.txt', done)
  expect(view.mutate).toHaveBeenCalledExactlyOnceWith({ name: 'renamed.txt' })
  view.complete()
  expect(done).toHaveBeenCalledExactlyOnceWith('renamed.txt')
  expect(popModal).not.toHaveBeenCalled()
})

it('closes after accepted completion and preserves valid Unicode names', async () => {
  const view = await modal('中文 name.txt')
  expect(view.mutate).toHaveBeenCalledExactlyOnceWith({ name: '中文 name.txt' })
  view.complete()
  expect(popModal).toHaveBeenCalledOnce()
})
