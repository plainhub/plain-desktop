import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
import LightboxInfo from '@/components/lightbox/LightboxInfo.vue'
import LightboxFileActionButtons from '@/components/lightbox/LightboxFileActionButtons.vue'
import InlineDeleteConfirm from '@/components/base/InlineDeleteConfirm.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import VOutlinedButton from '@/components/base/VOutlinedButton.vue'
import { click, clientTemplate, content, flush, nodes, renderView } from '../helpers/vue-node-renderer'

vi.mock('@/components/lightbox/LightboxMoveToFolders.vue', () => ({ default: { render: () => null } }))
vi.mock('@/hooks/media-trash', () => ({
  useMediaTrash: () => ({ trash: vi.fn(), trashLoading: () => false }),
  useMediaRestore: () => ({ restore: vi.fn(), restoreLoading: () => false }),
  useFileTrashState: () => ({ isTrashed: ref(false), canDelete: ref(true), canMove: ref(false), canTrash: ref(false) }),
}))
vi.mock('@/hooks/organize-undo', () => ({ useOrganizeUndo: () => ({ record: vi.fn() }) }))

clientTemplate(LightboxInfo, 'src/components/lightbox/LightboxInfo.vue')
clientTemplate(LightboxFileActionButtons, 'src/components/lightbox/LightboxFileActionButtons.vue')
clientTemplate(InlineDeleteConfirm, 'src/components/base/InlineDeleteConfirm.vue')
clientTemplate(VFilledButton, 'src/components/base/VFilledButton.vue')
clientTemplate(VOutlinedButton, 'src/components/base/VOutlinedButton.vue')

it('sends the desktop preview Delete shortcut through the same inline prompt', async () => {
  const listener = vi.fn()
  const deleteFile = vi.fn(async () => true)
  const onDeleteFile = vi.fn()
  const empty = { render: () => null }
  vi.stubGlobal('document', { addEventListener: listener, removeEventListener: vi.fn() })
  const view = renderView(LightboxInfo, {
    current: { path: '/fixture/file.pdf', name: 'file.pdf' }, deleteFile, onDeleteFile, urlTokenKey: '', tagsMap: new Map(), imageQuality: 'fast',
  }, {
    InlineDeleteConfirm, LightboxFileActionButtons, VFilledButton, VOutlinedButton, VCircularProgress: empty,
    LightboxQualityDropdown: empty, LightboxKeyboardShortcuts: empty, LightboxFileDetails: empty, LightboxFileTags: empty,
  })
  try {
    const keydown = listener.mock.calls.find(([event]) => event === 'keydown')?.[1]
    expect(keydown).toBeTypeOf('function')
    keydown({ key: 'Delete', preventDefault() {} })
    await flush()
    expect(onDeleteFile).not.toHaveBeenCalled()
    expect(deleteFile).not.toHaveBeenCalled()
    expect(content(view.root)).toContain('confirm_to_delete_name')
    click(nodes(view.root).find(node => node.tag === 'button' && content(node) === 'confirm')!)
    await flush()
    expect(deleteFile).toHaveBeenCalledOnce()
  } finally {
    view.unmount()
    vi.unstubAllGlobals()
  }
})
