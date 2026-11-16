import { createApp, h, nextTick, ref } from 'vue'
import VPagination from '@/components/VPagination.vue'
import '@/styles/main.scss'

const start = location.search.includes('largest-range') ? Number.MAX_SAFE_INTEGER - 100 : 10_000
const current = ref(start)
createApp({
  render: () => h('div', [
    h(VPagination, { total: 5, page: 1, limit: 1, go: () => {} }),
    h(VPagination, { total: Number.MAX_SAFE_INTEGER, page: current.value, limit: 1, go: page => { current.value = page } }),
  ]),
}).mount('#app')

const pagers = document.querySelectorAll<HTMLElement>('.pagination')
const buttons = [...pagers[1].querySelectorAll<HTMLButtonElement>('button')]
const digits = buttons.filter(button => /^\d+$/.test(button.textContent!.trim()))
const first = digits[0].getBoundingClientRect()
const geometry = digits.map(button => {
  const label = document.createRange()
  label.selectNodeContents(button)
  const text = label.getBoundingClientRect()
  const box = button.getBoundingClientRect()
  return {
    page: button.textContent!.trim(), height: box.height, width: box.width,
    leftInset: text.left - box.left, rightInset: box.right - text.right,
    rounded: ['16px', '999px'].includes(getComputedStyle(button).borderTopLeftRadius),
  }
})
const largest = geometry.find(entry => entry.page === String(Number.MAX_SAFE_INTEGER))!
const intact = geometry.every(entry => entry.height === 32 && entry.leftInset >= 7 && entry.rightInset >= 7)
const compact = first.height === 32 && first.width === 32
const pagerBox = pagers[1].getBoundingClientRect()
const rows = new Set(buttons.map(button => button.getBoundingClientRect().top)).size
const wrapping = rows > 1 && pagers[1].scrollWidth <= pagers[1].clientWidth
const arrows = [buttons[0], buttons.at(-1)!].every(button => {
  const box = button.getBoundingClientRect()
  return !button.disabled && button.tabIndex === 0 && box.left >= pagerBox.left && box.right <= pagerBox.right
})
digits.find(button => button.textContent!.trim() === String(current.value + 1))!.click()
await nextTick()
const navigates = current.value === start + 1 && pagers[1].querySelector('.active')?.textContent?.trim() === String(start + 1)
const pass = intact && compact && wrapping && arrows && navigates && largest.width > largest.height && largest.rounded
document.body.dataset.result = pass ? 'pass' : 'fail'
document.body.dataset.detail = JSON.stringify({ largest, compact, wrapping, rows, arrows, navigates, clipped: geometry.filter(entry => entry.leftInset < 7 || entry.rightInset < 7).map(entry => entry.page) })
