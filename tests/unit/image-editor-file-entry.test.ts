import { afterEach, expect, it, vi } from 'vitest'
import { defineComponent } from 'vue'
import { File } from 'node:buffer'
import ImageEditorList from '@/views/image-editor/ImageEditorListView.vue'
import ImageDropZone from '@/views/image-editor/components/ImageDropZone.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import { clientTemplate, renderView, nodes, fire, click, flush, content } from '../helpers/vue-node-renderer'

vi.mock('@/lib/format', () => ({ formatTimeAgo: () => '' }))
vi.mock('@/views/image-editor/store/plain-app-store', () => ({ PlainAppProjectStore: class { list = async () => [] } }))
clientTemplate(ImageEditorList, 'src/views/image-editor/ImageEditorListView.vue')
clientTemplate(ImageDropZone, 'src/views/image-editor/components/ImageDropZone.vue')
clientTemplate(VFilledButton, 'src/components/base/VFilledButton.vue')
let unmount: (() => void) | undefined
afterEach(() => { unmount?.(); vi.restoreAllMocks(); vi.unstubAllGlobals() })

it.each(['chooser', 'drop', 'paste'])('waits for Start editing after %s and opens the latest selected file on that click', async entry => {
  const listeners = new Map<string, (event: unknown) => void>()
  vi.stubGlobal('document', { addEventListener: (name: string, callback: (event: unknown) => void) => listeners.set(name, callback), removeEventListener: vi.fn() })
  const open = vi.fn()
  vi.stubGlobal('window', { open })
  const createUrl = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:fixture-image')
  const stub = defineComponent({ render: () => null })
  const view = renderView(ImageEditorList, {}, { VFilledButton, VCircularProgress: stub, InlineDeleteConfirm: stub, VDropdownMenu: stub })
  unmount = view.unmount
  const select = (name: string) => {
    const file = new File(['synthetic image'], name, { type: 'image/png' })
    if (entry === 'chooser') fire(nodes(view.root).find(node => node.tag === 'input')!, 'onChange', { target: { files: [file] } })
    if (entry === 'drop') fire(nodes(view.root).find(node => node.props.class?.includes('drop-zone'))!, 'onDrop', { preventDefault() {}, dataTransfer: { files: [file] } })
    if (entry === 'paste') listeners.get('paste')!({ clipboardData: { items: [{ kind: 'file', type: 'image/png', getAsFile: () => file }] } })
    return file
  }
  select('first.png')
  const latest = select('latest image.png')
  await flush()
  expect(open).not.toHaveBeenCalled()
  expect(createUrl).not.toHaveBeenCalled()
  const start = nodes(view.root).find(node => node.tag === 'button' && content(node) === 'image_editor.start_editing')
  expect(start).toBeDefined()
  click(start!)
  expect(createUrl).toHaveBeenCalledExactlyOnceWith(latest)
  expect(open).toHaveBeenCalledExactlyOnceWith('/image-editor/new?src=blob%3Afixture-image&name=latest%20image.png', '_blank')
})
