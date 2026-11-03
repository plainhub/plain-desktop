import { expect, it, vi } from 'vitest'
import { createRenderer, createSSRApp, h, ref } from 'vue'
import { renderToString } from 'vue/server-renderer'
const state = vi.hoisted(() => ({ progress: (_value: any) => {}, toast: vi.fn(), initial: null as any }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/stores/main', () => ({ useMainStore: vi.fn() }))
vi.mock('@/stores/temp', () => ({ useTempStore: vi.fn() }))
vi.mock('@/hooks/contact-picker', () => ({ useContactPicker: vi.fn() }))
vi.mock('@/lib/api/query', () => ({ initQuery: ({ handle }: any) => { if (state.initial) handle({ scanProgress: state.initial }, '') }, scanProgressGQL: '', homeStatsGQL: vi.fn(), simsGQL: '' }))
vi.mock('@/lib/api/mutation', () => ({ initMutation: () => ({ mutate: vi.fn(), loading: ref(false) }), callGQL: '', pauseMediaScanGQL: '', resumeMediaScanGQL: '', stopMediaScanGQL: '', rebuildMediaIndexGQL: '' }))
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
