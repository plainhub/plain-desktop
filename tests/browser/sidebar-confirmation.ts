import { createApp, h, nextTick, ref } from 'vue'
import SidebarListItem from '@/components/SidebarListItem.vue'
import DeleteConfirmPopup from '@/components/base/DeleteConfirmPopup.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import '@/styles/main.scss'

const open = ref(false)
const confirming = ref(false)
const result = ref('Running')
const nearBottom = new URLSearchParams(location.search).has('bottom')
const app = createApp({
  render: () => h('main', [
    h('output', { role: 'status' }, result.value),
    h('ul', { class: 'nav', style: `position: fixed; left: 80px; width: 216px; ${nearBottom ? 'bottom: 16px' : 'top: 240px'}` }, [
      h(SidebarListItem, { title: 'Fixture tag', forceActions: open.value }, {
        end: () => h('span', { class: 'count' }, '1'),
        actions: () => h(VIconButton, { id: 'fixture-actions', onClick: () => { open.value = true } }, () => '⋯'),
      }),
    ]),
    h(DeleteConfirmPopup, {
      anchor: 'fixture-actions',
      modelValue: open.value,
      'onUpdate:modelValue': value => { open.value = value },
      confirming: confirming.value,
      name: 'Fixture tag',
      onCancel: () => { confirming.value = false },
    }, {
      menu: () => h('div', { class: 'dropdown-item', onClick: () => { confirming.value = true } }, 'Delete'),
    }),
  ]),
})
app.config.globalProperties.$t = (key: string) => key
app.mount('#app')

const trigger = document.querySelector<HTMLButtonElement>('#fixture-actions')!
try {
  if (trigger.getClientRects().length) throw new Error('Actions are visible on an inactive row')
  trigger.click()
  await nextTick()
  document.querySelector<HTMLElement>('.v-dropdown-menu .dropdown-item')!.click()
  await nextTick()

  const anchor = trigger.getBoundingClientRect()
  const popup = document.querySelector<HTMLElement>('.delete-confirm-popup')!.parentElement!.getBoundingClientRect()
  const gap = nearBottom ? anchor.top - popup.bottom : popup.top - anchor.bottom
  if (anchor.width === 0 || Math.abs(gap - 8) > 1 || Math.abs(popup.left - anchor.left) > 1) {
    throw new Error(`Confirmation at (${popup.left}, ${popup.top}) is detached from the trigger at (${anchor.left}, ${anchor.top}), gap ${gap}px`)
  }

  document.querySelector<HTMLButtonElement>('.delete-confirm-popup .v-outlined-button')!.click()
  await nextTick()
  if (!document.querySelector('.v-dropdown-menu .dropdown-item')) throw new Error('Cancel did not reopen the action menu')
  result.value = 'PASS: Confirmation stays next to its trigger after the pointer leaves the row'
} catch (error) {
  result.value = `FAIL: ${String(error)}`
} finally {
  open.value = false
  confirming.value = false
}
