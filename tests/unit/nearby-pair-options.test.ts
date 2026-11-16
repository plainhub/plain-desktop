import { expect, it, vi } from 'vitest'
import NearbyPairOptions from '@/views/chat/NearbyPairOptions.vue'
import VFilledButton from '@/components/base/VFilledButton.vue'
import VOutlinedButton from '@/components/base/VOutlinedButton.vue'
import { click, clientTemplate, content, nodes, renderView } from '../helpers/vue-node-renderer'

clientTemplate(NearbyPairOptions, 'src/views/chat/NearbyPairOptions.vue')
clientTemplate(VFilledButton, 'src/components/base/VFilledButton.vue')
clientTemplate(VOutlinedButton, 'src/components/base/VOutlinedButton.vue')

it('uses a filled Continue and outlined Cancel without changing the selected mode', () => {
  const onConfirm = vi.fn()
  const onCancel = vi.fn()
  const view = renderView(NearbyPairOptions, { onConfirm, onCancel }, {
    VFilledButton, VOutlinedButton, VCircularProgress: { render: () => null },
  })
  try {
    const button = (label: string) => nodes(view.root).find(node => node.tag === 'button' && content(node) === label)!
    expect(button('pairing_start').props.class).toBe('v-filled-button')
    expect(button('cancel').props.class).toBe('v-outlined-button')
    click(button('pairing_start'))
    click(button('cancel'))
    expect(onConfirm).toHaveBeenCalledExactlyOnceWith('control')
    expect(onCancel).toHaveBeenCalledOnce()
  } finally {
    view.unmount()
  }
})
