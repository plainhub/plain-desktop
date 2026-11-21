import { expect, it, vi } from 'vitest'
import CallActionButtons from '@/views/calls/CallActionButtons.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import VOutlinedButton from '@/components/base/VOutlinedButton.vue'
import VDropdownMenu from '@/components/base/VDropdownMenu.vue'
import DeleteConfirmPopup from '@/components/base/DeleteConfirmPopup.vue'
import { DataType } from '@/lib/data'
import { click, clientTemplate, content, flush, nodes, renderView } from '../helpers/vue-node-renderer'

for (const [component, file] of [
  [CallActionButtons, 'src/views/calls/CallActionButtons.vue'],
  [VIconButton, 'src/components/base/VIconButton.vue'],
  [VFilledButton, 'src/components/base/VFilledButton.vue'],
  [VOutlinedButton, 'src/components/base/VOutlinedButton.vue'],
  [VDropdownMenu, 'src/components/base/VDropdownMenu.vue'],
  [DeleteConfirmPopup, 'src/components/base/DeleteConfirmPopup.vue'],
] as const) clientTemplate(component, file)

it('keeps the call controls in the row while deletion is confirmed in the Files popup', async () => {
  const item = { id: 'fixture-call', number: '5550100', tags: [] }
  const onDeleteItem = vi.fn()
  const view = renderView(CallActionButtons, { item, tags: [], dataType: DataType.CALL, onDeleteItem }, {
    VIconButton, VFilledButton, VOutlinedButton, VDropdownMenu, DeleteConfirmPopup, VCircularProgress: { render: () => null }, TagRelationsDropdown: { render: () => null },
  })
  const deleteButton = () => nodes(view.root).find(node =>
    node.tag === 'button' && nodes(node).some(icon => icon.tag === 'i-material-symbols:delete-forever-outline-rounded'),
  )
  const popup = () => nodes(view.portal).find(node => node.props.class?.includes('is-open'))
  const popupButton = (label: string) => nodes(popup()!).find(node => node.tag === 'button' && content(node) === label)!
  vi.stubGlobal('document', { addEventListener: vi.fn(), removeEventListener: vi.fn(), getElementById: vi.fn() })
  try {
    click(deleteButton()!)
    await flush()
    expect(popup()).toBeDefined()
    expect(deleteButton()).toBeDefined()
    expect(nodes(view.root).some(node => node.tag === 'i-material-symbols:call-outline-rounded')).toBe(true)
    expect(onDeleteItem).not.toHaveBeenCalled()
    click(popupButton('cancel'))
    await flush()
    expect(popup()).toBeUndefined()
    expect(onDeleteItem).not.toHaveBeenCalled()
    click(deleteButton()!)
    await flush()
    click(popupButton('confirm'))
    await flush()
    expect(onDeleteItem).toHaveBeenCalledExactlyOnceWith(item)
    expect(popup()).toBeUndefined()
  } finally {
    view.unmount()
    vi.unstubAllGlobals()
  }
})
