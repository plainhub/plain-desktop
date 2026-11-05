import { expect, it, vi } from 'vitest'
import { createRenderer, createSSRApp, h, ref } from 'vue'
import { renderToString } from 'vue/server-renderer'
const state = vi.hoisted(() => ({ progress: (_value: any) => {}, toast: vi.fn(), mutation: vi.fn(), initial: null as any }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/stores/main', () => ({ useMainStore: vi.fn() }))
vi.mock('@/stores/temp', () => ({ useTempStore: vi.fn() }))
vi.mock('@/hooks/contact-picker', () => ({ useContactPicker: vi.fn() }))
vi.mock('@/lib/api/query', () => ({ initQuery: ({ handle }: any) => { if (state.initial) handle({ scanProgress: state.initial }, '') }, scanProgressGQL: '', homeStatsGQL: vi.fn(), simsGQL: '' }))
vi.mock('@/lib/api/mutation', () => ({ initMutation: ({ document }: any) => ({ mutate: () => state.mutation(document), loading: ref(false) }), callGQL: '', pauseMediaScanGQL: 'pauseMediaScan', resumeMediaScanGQL: 'resumeMediaScan', stopMediaScanGQL: 'stopMediaScan', rebuildMediaIndexGQL: 'rebuildMediaIndex' }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe: (_field: string, callback: (p: any) => void) => { state.progress = callback; return vi.fn() } } }))
vi.mock('@/components/toaster', () => ({ default: state.toast }))
import { useScanAction } from '@/views/home/home'

it('reports fast completion and exposes failure with a retry without inventing a running timer', () => {
  let scan: ReturnType<typeof useScanAction>
  const renderer = createRenderer<any, any>({
    insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
    setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
  })
  const app = renderer.createApp({ setup() { scan = useScanAction(); return () => h('div') } })
  app.mount({})
  try {
    expect(state.toast).not.toHaveBeenCalled()
    state.progress({ state: 'RUNNING', indexed: 0, total: null, pending: null })
    expect(scan!.scanActive.value).toBe(true)
    expect(scan!.stateLabel.value).toBe('counting_files')
    state.progress({ state: 'RUNNING', indexed: 1234, total: null, pending: null })
    expect(scan!.scanProgress.value.indexed).toBe(1234)
    expect(scan!.percent.value).toBe(0)
    state.progress({ state: 'PAUSED', indexed: 1234, total: null, pending: null })
    expect(scan!.stateLabel.value).toBe('paused')
    expect(scan!.scanProgress.value.indexed).toBe(1234)
    state.progress({ state: 'IDLE', indexed: 0, total: 0, pending: 0 })
    expect(state.toast).toHaveBeenCalledWith('index_state_completed')
    state.progress({ state: 'FAILED', indexed: 0, total: null, pending: null, error: 'Scan failed' })
    expect(scan!.showRebuild.value).toBe(true)
    expect(scan!.stateLabel.value).toBe('Scan failed')
    expect(state.toast).toHaveBeenCalledWith('Scan failed', 'error')
  } finally { app.unmount() }
})

it('renders the scanner failure in the existing status row after the scan stops', async () => {
  const { default: ScanPanel } = await import('@/views/home/ScanPanel.vue')
  state.initial = { state: 'FAILED', indexed: 0, total: null, pending: null, error: 'Cannot read scan source' }
  const Button = { setup: (_props: unknown, { slots }: any) => () => h('button', slots.default?.()) }
  const app = createSSRApp(ScanPanel)
  app.config.globalProperties.$t = (key: string) => key
  app.component('v-outlined-button', Button)
  app.component('v-filled-button', Button)
  app.component('v-circular-progress', { render: () => h('progress') })
  try {
    const html = await renderToString(app)
    expect(html).toContain('Cannot read scan source')
    expect(html).toContain('rebuild_index')
  } finally { state.initial = null }
})

it.each([
  { state: 'RUNNING', indexed: 1234, total: null, pending: null, label: 'building_file_index', count: '1,234' },
  { state: 'RUNNING', indexed: 0, total: null, pending: null, label: 'counting_files', count: null },
  { state: 'PAUSED', indexed: 1234, total: null, pending: null, label: 'paused', count: '1,234' },
  { state: 'PAUSED', indexed: 0, total: null, pending: null, label: 'paused', count: null },
  { state: 'RUNNING', indexed: 1234, total: 2000, pending: 766, label: 'building_file_index', count: '1,234 / 2,000' },
  { state: 'RUNNING', indexed: 0, total: 2, pending: 2, label: 'building_file_index', count: '0 / 2' },
  { state: 'RUNNING', indexed: 0, total: 0, pending: 0, label: 'building_file_index', count: '0 / 0' },
])('shows processed files for $state with total $total', async ({ label, count, ...progress }) => {
  const { default: ScanPanel } = await import('@/views/home/ScanPanel.vue')
  state.initial = progress
  const Button = { setup: (_props: unknown, { slots }: any) => () => h('button', slots.default?.()) }
  const app = createSSRApp(ScanPanel)
  app.config.globalProperties.$t = (key: string) => key
  app.component('v-outlined-button', Button)
  app.component('v-filled-button', Button)
  try {
    const html = await renderToString(app)
    expect(html).toContain(label)
    if (count === null) expect(html).not.toContain('class="muted"')
    else expect(html.replace(/<!--.*?-->/g, '').replace(/\s+/g, ' ')).toContain(count)
    if (label !== 'counting_files') expect(html).not.toContain('counting_files')
    if (progress.total === null) {
      expect(html).not.toMatch(/>\d+%</)
      expect(html).not.toContain('pending')
    } else if (progress.pending !== null && progress.pending > 0) {
      expect(html).toContain(progress.indexed === 0 ? '0%' : '62%')
      expect(html).toContain(`pending ${progress.pending}`)
    }
  } finally { state.initial = null }
})


it('the Stop handler sends only stopMediaScan when the Rebuild button appears', async () => {
  const { default: ScanPanel } = await import('@/views/home/ScanPanel.vue')
  state.initial = { state: 'RUNNING', indexed: 1, total: 10, pending: 9 }
  state.mutation.mockClear()
  const actions: { label: string; click: (event: any) => Promise<void> }[] = []
  const Button = {
    setup: (_props: unknown, { attrs, slots }: any) => {
      const content = slots.default?.()
      if (attrs.onClick) actions.push({ label: content.map((node: any) => node.children).join(''), click: attrs.onClick })
      return () => h('button', content)
    },
  }
  const render = async () => {
    const app = createSSRApp(ScanPanel)
    app.config.globalProperties.$t = (key: string) => key
    app.component('v-outlined-button', Button)
    app.component('v-filled-button', Button)
    return renderToString(app)
  }
  try {
    await render()
    const stop = actions.find(action => action.label === 'stop')
    expect(stop).toBeDefined()
    await stop!.click({})
    state.initial = { state: 'STOPPED', indexed: 1, total: 10, pending: 9 }
    expect(await render()).toContain('rebuild_index')
    expect(state.mutation).toHaveBeenCalledExactlyOnceWith('stopMediaScan')
  } finally { state.initial = null }
})
