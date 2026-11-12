import { expect, it } from 'vitest'
import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import DeveloperDatabaseSidebar from '@/views/developer/DeveloperDatabaseSidebar.vue'

async function sidebar(helpTable = '') {
  const component = DeveloperDatabaseSidebar as any
  let state: any
  const app = createSSRApp({ ...component, setup(props: unknown, context: unknown) {
    state = component.setup(props, context)
    state.helpTable.value = helpTable
    return state
  } }, { tables: [{ name: 'notes', createSql: 'CREATE TABLE notes (id TEXT)', columns: [] }, { name: 'feeds', createSql: sql, columns: [] }], activeTable: 'notes', loading: false })
  app.config.globalProperties.$t = (key: string) => key
  app.directive('tooltip', {})
  for (const name of ['Sidebar2', 'v-icon-button', 'v-filled-button']) {
    app.component(name, { setup: (_: unknown, { slots }: any) => () => h('div', slots.default?.()) })
  }
  app.component('v-dropdown', { props: ['modelValue'], setup: (props: any, { slots }: any) => () => h('div', [slots.trigger?.(), props.modelValue ? slots.default?.() : null]) })
  app.component('v-circular-progress', { render: () => null })
  app.component('NoDataPlaceholder', { render: () => null })
  const html = await renderToString(app)
  return { state, html }
}

it('renders tables with selected state and individually labelled column help', async () => {
  const { html } = await sidebar()
  expect(html).toContain('table-item selectable-card selected')
  expect(html).toMatch(/aria-pressed="true"[^>]*>notes<\/button>/)
  expect(html).toMatch(/aria-pressed="false"[^>]*>feeds<\/button>/)
  expect(html).toContain('aria-label="feeds: CREATE TABLE"')
})

const sql = `CREATE TABLE audio_queue_items (
    file_id TEXT NOT NULL PRIMARY KEY CHECK(length(file_id)>0),
    sort_order INTEGER NOT NULL CHECK(sort_order>=0)
)`

it('shows the original table SQL including CHECK constraints for an unselected table', async () => {
  const { html } = await sidebar('feeds')
  expect(html).toContain('CHECK(length(file_id)&gt;0)')
  expect(html).toContain('CHECK(sort_order&gt;=0)')
})

it('shows only the requested table schema', async () => {
  const { html } = await sidebar('notes')
  expect(html).toContain('CREATE TABLE notes (id TEXT)')
  expect(html).not.toContain('CREATE TABLE audio_queue_items')
})
