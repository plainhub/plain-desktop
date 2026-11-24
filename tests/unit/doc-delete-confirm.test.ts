import { expect, it, vi } from 'vitest'
import DocActionButtons from '@/views/docs/DocActionButtons.vue'
import VDropdownMenu from '@/components/base/VDropdownMenu.vue'
import DeleteConfirmPopup from '@/components/base/DeleteConfirmPopup.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import VOutlinedButton from '@/components/base/VOutlinedButton.vue'
import { DataType } from '@/lib/data'
import { click, clientTemplate, content, flush, nodes, renderView } from '../helpers/vue-node-renderer'

clientTemplate(DocActionButtons, 'src/views/docs/DocActionButtons.vue')
clientTemplate(VDropdownMenu, 'src/components/base/VDropdownMenu.vue')
clientTemplate(DeleteConfirmPopup, 'src/components/base/DeleteConfirmPopup.vue')
clientTemplate(VIconButton, 'src/components/base/VIconButton.vue')
clientTemplate(VFilledButton, 'src/components/base/VFilledButton.vue')
clientTemplate(VOutlinedButton, 'src/components/base/VOutlinedButton.vue')

it('confirms permanent deletion in the anchored Files popup and preserves the document controls', async () => {
  const item = { id: 'fixture-doc', title: 'Fixture.txt', path: '/fixture/Fixture.txt', tags: [] }
  const deleteItem = vi.fn(async () => true)
  const onDeleteItem = vi.fn()
  const empty = { render: () => null }
  const view = renderView(DocActionButtons, {
    item, filter: { trash: true }, dataType: DataType.DOC, tags: [],
    trash: vi.fn(), restore: vi.fn(), restoreLoading: () => false, trashLoading: () => false, deleteItem, onDeleteItem,
  }, {
    VIconButton, VFilledButton, VOutlinedButton, DeleteConfirmPopup, VDropdownMenu, VDropdown: empty, TagRelationsDropdown: empty, VCircularProgress: empty,
  })
  const open = () => click(nodes(view.root).find(node => nodes(node).some(icon => icon.tag === 'i-material-symbols:delete-forever-outline-rounded') && node.tag === 'button')!)
  const button = (label: string) => nodes(view.portal).find(node => node.tag === 'button' && content(node) === label)!
  vi.stubGlobal('document', { addEventListener: vi.fn(), removeEventListener: vi.fn(), getElementById: vi.fn() })
  try {
    open()
    await flush()
    expect(onDeleteItem).not.toHaveBeenCalled()
    expect(deleteItem).not.toHaveBeenCalled()
    expect(content(view.portal)).toContain('confirm_to_delete_name')
    expect(nodes(view.root).some(node => node.props['aria-label'] === 'delete')).toBe(true)
    click(button('cancel'))
    await flush()
    expect(deleteItem).not.toHaveBeenCalled()
    open()
    await flush()
    click(button('confirm'))
    await flush()
    expect(deleteItem).toHaveBeenCalledExactlyOnceWith(item)
    expect(content(view.portal)).not.toContain('confirm_to_delete_name')
  } finally { view.unmount(); vi.unstubAllGlobals() }
})
