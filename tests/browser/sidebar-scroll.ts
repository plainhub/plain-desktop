import { createApp, h, nextTick, onMounted, ref } from 'vue'
import { createPinia } from 'pinia'
import Sidebar2 from '@/components/Sidebar2.vue'
import DatabaseSidebar from '@/views/developer/DeveloperDatabaseSidebar.vue'
import VirtualList from '@/components/virtualscroll'
import '@/styles/main.scss'

const tables = Array.from({ length: 80 }, (_, index) => `table_${index + 1}`)
const feeds = tables.map((_, index) => ({ id: index + 1 }))
const frame = () => new Promise<void>(resolve => requestAnimationFrame(() => resolve()))

const app = createApp({
  setup() {
    const result = ref('Running')
    const height = ref('calc(100vh - 104px)')
    async function verify() {
      try {
        for (const value of ['calc(100vh - 104px)', '320px']) {
          height.value = value
          await nextTick()
          await frame()
          for (const list of document.querySelectorAll<HTMLElement>('.tables-list, .feeds-list')) {
            list.scrollTop = list.scrollHeight
          }
          await frame()
          await frame()
          for (const panel of document.querySelectorAll<HTMLElement>('.test-panel')) {
            const list = panel.querySelector<HTMLElement>('.tables-list, .feeds-list')!
            const items = list.querySelectorAll<HTMLElement>('.table-item, .feed-item')
            const last = items[items.length - 1]
            const expected = panel.dataset.name === 'Database' ? 'table_80' : 'Feed 80'
            if (!last || last.textContent?.trim() !== expected || Math.abs(list.scrollHeight - list.clientHeight - list.scrollTop) > 1) {
              throw new Error(`${panel.dataset.name}: list did not reach its last item`)
            }
            if (last.getBoundingClientRect().bottom > panel.getBoundingClientRect().bottom + 1) {
              throw new Error(`${panel.dataset.name}: last item is clipped at ${value}`)
            }
          }
        }
        result.value = 'PASS: Database and Feeds last items remain visible at both parent heights'
      } catch (error) {
        result.value = `FAIL: ${String(error)}`
      }
    }
    onMounted(verify)
    return () => h('div', [
      h('div', { role: 'status', style: 'height:48px' }, result.value),
      h('button', { onClick: verify }, 'Run again'),
      h('div', { style: `height:${height.value};display:flex;overflow:hidden` }, [
        h('section', { class: 'test-panel', 'data-name': 'Database', style: 'display:flex;width:50%;min-height:0;overflow:hidden' }, [
          h(DatabaseSidebar, { tables, activeTable: tables[0], loading: false }),
        ]),
        h('section', { class: 'test-panel', 'data-name': 'Feeds', style: 'display:flex;width:50%;min-height:0;overflow:hidden' }, [
          h(Sidebar2, { detail: true }, { default: () => [
            h('div', { class: 'top-app-bar' }, 'Feeds'),
            h(VirtualList, { class: 'scroller feeds-list', dataKey: 'id', dataSources: feeds, estimateSize: 56 }, {
              item: ({ item }: { item: { id: number } }) => h('button', { class: 'feed-item', style: 'display:block;width:100%;height:56px' }, `Feed ${item.id}`),
            }),
          ] }),
        ]),
      ]),
    ])
  },
})
app.use(createPinia())
app.config.globalProperties.$t = (key: string) => key
app.directive('tooltip', {})
app.mount('#app')
