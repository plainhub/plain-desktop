import { expect, it, vi } from 'vitest'
import DocActionButtons from '@/views/docs/DocActionButtons.vue'
import InlineDeleteConfirm from '@/components/base/InlineDeleteConfirm.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import VOutlinedButton from '@/components/base/VOutlinedButton.vue'
import { DataType } from '@/lib/data'
import { click, clientTemplate, content, flush, nodes, renderView } from '../helpers/vue-node-renderer'

vi.mock('@/lib/feature', () => ({
  hasMediaDelete: () => true,
  hasMediaTrash: (app: { capabilities: string[] }) => app.capabilities.includes('trash'),
}))

clientTemplate(DocActionButtons, 'src/views/docs/DocActionButtons.vue')
clientTemplate(InlineDeleteConfirm, 'src/components/base/InlineDeleteConfirm.vue')
clientTemplate(VIconButton, 'src/components/base/VIconButton.vue')
clientTemplate(VFilledButton, 'src/components/base/VFilledButton.vue')
clientTemplate(VOutlinedButton, 'src/components/base/VOutlinedButton.vue')

it('confirms permanent deletion next to a document without opening a second surface', async () => {
  const item = { id: 'fixture-doc', title: 'Fixture.txt', path: '/fixture/Fixture.txt', tags: [] }
  const deleteItem = vi.fn(async () => true)
  const onDeleteItem = vi.fn()
  const empty = { render: () => null }
  const view = renderView(DocActionButtons, {
    item, app: { capabilities: [] }, filter: { trash: false }, dataType: DataType.DOC, tags: [],
    trash: vi.fn(), restore: vi.fn(), restoreLoading: () => false, trashLoading: () => false, deleteItem, onDeleteItem,
  }, {
    VIconButton, VFilledButton, VOutlinedButton, InlineDeleteConfirm, VDropdown: empty, TagRelationsDropdown: empty, VCircularProgress: empty,
  })
  const open = () => click(nodes(view.root).find(node => nodes(node).some(icon => icon.tag === 'i-material-symbols:delete-forever-outline-rounded') && node.tag === 'button')!)
  const button = (label: string) => nodes(view.root).find(node => node.tag === 'button' && content(node) === label)!
  try {
    open()
    await flush()
    expect(onDeleteItem).not.toHaveBeenCalled()
    expect(deleteItem).not.toHaveBeenCalled()
    expect(content(view.root)).toContain('confirm_to_delete_name')
    click(button('cancel'))
    await flush()
    expect(deleteItem).not.toHaveBeenCalled()
    open()
    await flush()
    click(button('confirm'))
    await flush()
    expect(deleteItem).toHaveBeenCalledExactlyOnceWith(item)
    expect(content(view.root)).not.toContain('confirm_to_delete_name')
  } finally { view.unmount() }
})
