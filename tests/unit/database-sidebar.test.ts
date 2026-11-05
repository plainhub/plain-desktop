import { afterEach, expect, it, vi } from 'vitest'
import { createSSRApp, h, nextTick } from 'vue'
import { renderToString } from 'vue/server-renderer'
import DeveloperDatabaseSidebar from '@/views/developer/DeveloperDatabaseSidebar.vue'

const { gqlFetch } = vi.hoisted(() => ({ gqlFetch: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch, GqlError: class extends Error {} }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { onAttached: () => () => {} } }))
afterEach(() => { vi.clearAllMocks() })

async function sidebar() {
  const component = DeveloperDatabaseSidebar as any
  let state: any
  const app = createSSRApp({ ...component, setup(props: unknown, context: unknown) {
    state = component.setup(props, context)
    return state
  } }, { tables: ['notes', 'feeds'], activeTable: 'notes', loading: false })
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
  gqlFetch.mockResolvedValueOnce({ data: { dbTableInfo: { idKey: 'file_id', createSql: sql } } })
  const { state } = await sidebar()
  state.showTableSchema('feeds')
  await nextTick()
  expect(gqlFetch.mock.calls[0][1]).toEqual({ table: 'feeds' })
  expect(state.helpTable.value).toBe('feeds')
  expect(state.createSql.value).toBe(sql)
  expect(state.schemaError.value).toBe('')
})

it('discards older schema responses after switching tables', async () => {
  let resolveFirst!: (value: unknown) => void
  gqlFetch.mockImplementationOnce(() => new Promise(resolve => { resolveFirst = resolve }))
  gqlFetch.mockResolvedValueOnce({ data: { dbTableInfo: { createSql: sql } } })
  const { state } = await sidebar()
  state.showTableSchema('notes')
  state.showTableSchema('feeds')
  await nextTick()
  resolveFirst({ data: { dbTableInfo: { createSql: 'outdated SQL' } } })
  await nextTick()
  expect(state.createSql.value).toBe(sql)
})

it('clears previous SQL and shows query errors instead of stale schema', async () => {
  gqlFetch.mockResolvedValueOnce({ data: { dbTableInfo: { createSql: sql } } })
  gqlFetch.mockResolvedValueOnce({ errors: [{ message: 'permission_denied' }] })
  const { state } = await sidebar()
  state.showTableSchema('notes')
  await nextTick()
  state.showTableSchema('feeds')
  await nextTick()
  expect(state.createSql.value).toBe('')
  expect(state.schemaError.value).toBe('permission_denied')
})
