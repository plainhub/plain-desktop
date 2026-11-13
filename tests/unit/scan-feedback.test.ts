import { expect, it, vi } from 'vitest'
import { createRenderer, createSSRApp, h, ref, ssrContextKey } from 'vue'
import { renderToString } from 'vue/server-renderer'
const state = vi.hoisted(() => ({ library: (_value: any) => {}, reload: vi.fn(), progress: (_value: any) => {}, toast: vi.fn(), mutation: vi.fn(), pushModal: vi.fn(), initial: null as any, volumes: [] as any[] }))
vi.mock('@/lib/api/time-sync', () => ({ wrapWithReplayProtection: (value: string) => value }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/stores/main', () => ({ useMainStore: vi.fn() }))
vi.mock('@/stores/temp', () => ({ useTempStore: vi.fn() }))
vi.mock('@/hooks/contact-picker', () => ({ useContactPicker: vi.fn() }))
vi.mock('@/lib/api/query', () => ({
  initQuery: ({ document, handle }: any) => {
    if (document === '') {
      if (state.initial) handle({ scanProgress: state.initial }, '')
    } else handle({ fileIndexVolumes: state.volumes, fileIndexDefaults: { excludedDirs: [], excludedNames: [], excludedPackages: [], excludedUserLibraries: false } }, '')
    return { loading: ref(false), refetch: document === '' ? vi.fn() : state.reload }
  },
  scanProgressGQL: '',
  homeStatsGQL: vi.fn(),
  simsGQL: '',
}))
vi.mock('@/lib/api/mutation', () => ({
  initMutation: ({ document }: any) => ({ mutate: () => state.mutation(document), loading: ref(false) }),
  callGQL: '',
  pauseMediaScanGQL: 'pauseMediaScan',
  resumeMediaScanGQL: 'resumeMediaScan',
  stopMediaScanGQL: 'stopMediaScan',
  rebuildMediaIndexGQL: 'rebuildMediaIndex',
}))
vi.mock('@/lib/api/subscription-client', () => ({
  graphqlSubscriptions: {
    subscribe: (_field: string, callback: (p: any) => void) => {
      if (_field === 'mediaUpdated') state.library = callback
      else state.progress = callback
      return vi.fn()
    },
  },
}))
vi.mock('@/components/toaster', () => ({ default: state.toast }))
vi.mock('@/components/modal', () => ({ pushModal: state.pushModal }))
import { useScanAction } from '@/views/home/home'

it('keeps completion silent and exposes failures with a retry without inventing a running timer', () => {
  let scan: ReturnType<typeof useScanAction>
  const renderer = createRenderer<any, any>({
    insert: vi.fn(),
    remove: vi.fn(),
    createElement: () => ({}),
    createText: () => ({}),
    createComment: () => ({}),
    setText: vi.fn(),
    setElementText: vi.fn(),
    parentNode: () => null,
    nextSibling: () => null,
    patchProp: vi.fn(),
  })
  const app = renderer.createApp({
    setup() {
      scan = useScanAction()
      return () => h('div')
    },
  })
  app.provide(ssrContextKey, { modules: new Set() })
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
    expect(state.toast).not.toHaveBeenCalled()
    state.progress({ state: 'FAILED', indexed: 0, total: null, pending: null, error: 'Scan failed' })
    expect(scan!.showRebuild.value).toBe(true)
    expect(scan!.stateLabel.value).toBe('Scan failed')
    expect(state.toast).toHaveBeenCalledWith('Scan failed', 'error')
  } finally {
    app.unmount()
  }
})

it('renders the scanner failure in the existing status row after the scan stops', async () => {
  const { default: FileIndexCard } = await import('@/views/home/file-index/FileIndexCard.vue')
  state.initial = { state: 'FAILED', indexed: 0, total: null, pending: null, error: 'Cannot read scan source' }
  const Button = {
    setup:
      (_props: unknown, { slots }: any) =>
      () =>
        h('button', slots.default?.()),
  }
  const app = createSSRApp(FileIndexCard, { mounts: [] })
  app.config.globalProperties.$t = (key: string) => key
  app.component('VIconButton', Button)
  app.component('VTextButton', Button)
  app.directive('tooltip', {})
  app.component('VFilledButton', Button)
  app.component('VCircularProgress', { render: () => h('progress') })
  try {
    const html = await renderToString(app)
    expect(html).toContain('Cannot read scan source')
    expect(html).not.toContain('rebuild_index')
  } finally {
    state.initial = null
  }
})

it.each([
  { state: 'RUNNING', indexed: 1234, total: null, pending: null, label: 'building_file_index', count: '1,234' },
  { state: 'RUNNING', indexed: 0, total: null, pending: null, label: 'counting_files', count: null },
  { state: 'PAUSED', indexed: 1234, total: null, pending: null, label: 'paused', count: '1,234' },
  { state: 'PAUSED', indexed: 0, total: null, pending: null, label: 'paused', count: null },
  { state: 'RUNNING', indexed: 1234, total: 2000, pending: 766, label: 'building_file_index', count: '1,234 / 2,000' },
  { state: 'RUNNING', indexed: 0, total: 2, pending: 2, label: 'building_file_index', count: '0 / 2' },
  { state: 'RUNNING', indexed: 0, total: 0, pending: 0, label: 'building_file_index', count: null },
])('shows processed files for $state with total $total', async ({ label, count, ...progress }) => {
  const { default: FileIndexCard } = await import('@/views/home/file-index/FileIndexCard.vue')
  state.initial = progress
  const Button = {
    setup:
      (_props: unknown, { slots }: any) =>
      () =>
        h('button', slots.default?.()),
  }
  const app = createSSRApp(FileIndexCard, { mounts: [] })
  app.config.globalProperties.$t = (key: string) => key
  app.component('VIconButton', Button)
  app.component('VTextButton', Button)
  app.directive('tooltip', {})
  app.component('VFilledButton', Button)
  try {
    const html = await renderToString(app)
    expect(html).toContain(label)
    if (count === null) expect(html).not.toMatch(/>0(?: \/ 0)?</)
    else expect(html.replace(/<!--.*?-->/g, '').replace(/\s+/g, ' ')).toContain(count)
    if (label !== 'counting_files') expect(html).not.toContain('counting_files')
    if (progress.total === null) {
      expect(html).not.toMatch(/>\d+%</)
      expect(html).not.toContain('pending')
    } else if (progress.pending !== null && progress.pending > 0) {
      expect(html).toContain(progress.indexed === 0 ? '0%' : '62%')
      expect(html).not.toContain('pending')
    }
  } finally {
    state.initial = null
  }
})

it('the Stop handler sends only stopMediaScan when the Rebuild button appears', async () => {
  const { default: FileIndexCard } = await import('@/views/home/file-index/FileIndexCard.vue')
  state.volumes = [0, 1].map((id) => ({ id: String(id), path: `/drive-${id}`, isAvailable: true, indexMode: 'BLOCKLIST', includedDirs: [], excludedDirs: [] }))
  state.initial = { state: 'RUNNING', indexed: 1, total: 10, pending: 9 }
  state.mutation.mockClear()
  const actions: { label: string; click: (event: any) => Promise<void> }[] = []
  const Button = {
    setup: (_props: unknown, { attrs, slots }: any) => {
      const content = slots.default?.()
      if (attrs.onClick) actions.push({ label: attrs['aria-label'], click: attrs.onClick })
      return () => h('button', content)
    },
  }
  const render = async () => {
    const app = createSSRApp(FileIndexCard, { mounts: [] })
    app.config.globalProperties.$t = (key: string) => key
    app.component('VIconButton', Button)
    app.component('VTextButton', Button)
    app.directive('tooltip', {})
    app.component('VFilledButton', Button)
    return renderToString(app)
  }
  try {
    await render()
    const stop = actions.find((action) => action.label === 'stop')
    expect(stop).toBeDefined()
    await stop!.click({})
    state.initial = { state: 'STOPPED', indexed: 1, total: 10, pending: 9 }
    expect(await render()).toContain('rebuild_index')
    expect(state.mutation).toHaveBeenCalledExactlyOnceWith('stopMediaScan')
  } finally {
    state.initial = null
    state.volumes = []
  }
})

it('coalesces library invalidations and cancels pending refreshes on unmount', async () => {
  vi.useFakeTimers()
  state.reload.mockClear()
  const { default: FileIndexCard } = await import('@/views/home/file-index/FileIndexCard.vue')
  const renderer = createRenderer<any, any>({
    insert: vi.fn(),
    remove: vi.fn(),
    createElement: () => ({}),
    createText: () => ({}),
    createComment: () => ({}),
    setText: vi.fn(),
    setElementText: vi.fn(),
    parentNode: () => null,
    nextSibling: () => null,
    patchProp: vi.fn(),
  })
  const app = renderer.createApp(FileIndexCard, { mounts: [] })
  const Button = {
    setup:
      (_props: unknown, { slots }: any) =>
      () =>
        h('button', slots.default?.()),
  }
  app.config.globalProperties.$t = (key: string) => key
  app.component('VIconButton', Button)
  app.component('VTextButton', Button)
  app.directive('tooltip', {})
  app.provide(ssrContextKey, { modules: new Set() })
  app.mount({})
  try {
    for (let i = 0; i < 100; i++) state.library({ types: ['AUDIO', 'VIDEO', 'IMAGE'] })
    expect(state.reload).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(2000)
    expect(state.reload).toHaveBeenCalledOnce()
    state.library({ types: ['AUDIO'] })
    app.unmount()
    await vi.advanceTimersByTimeAsync(10000)
    expect(state.reload).toHaveBeenCalledOnce()
  } finally {
    app.unmount()
    vi.useRealTimers()
  }
})

it.each([
  [1, 0],
  [3, 0],
  [3, 1],
])('offers the appropriate index action with %i volumes and %i configured', async (count, configured) => {
  const { default: FileIndexCard } = await import('@/views/home/file-index/FileIndexCard.vue')
  const { default: FileIndexScopeDialog } = await import('@/views/home/file-index/FileIndexScopeDialog.vue')
  state.initial = { state: 'IDLE', indexed: 0, total: 0, pending: 0 }
  state.volumes = Array.from({ length: count }, (_, id) => ({
    id: String(id), path: `/drive-${id}`, isAvailable: true, indexMode: id < configured ? 'BLOCKLIST' : 'UNCONFIGURED', includedDirs: [], excludedDirs: [],
  }))
  state.pushModal.mockClear()
  state.mutation.mockClear()
  const createActions: (() => void)[] = []
  const app = createSSRApp(FileIndexCard, { mounts: [] })
  app.config.globalProperties.$t = (key: string) => key
  const Button = {
    setup: (_props: unknown, { attrs, slots }: any) => {
      const content = slots.default?.()
      if (content?.[0]?.children === 'file_index.create' && !attrs.disabled) createActions.push(attrs.onClick)
      return () => h('button', content)
    },
  }
  app.component('VTextButton', Button)
  app.component('VIconButton', Button)
  app.directive('tooltip', {})
  try {
    const html = await renderToString(app)
    expect(html).not.toContain('class="summary"')
    expect(html).not.toContain('file_index.unconfigured')
    expect(createActions).toHaveLength(count - configured)
    expect(html.match(/rebuild_index/g)?.length ?? 0).toBe(configured ? configured + 1 : 0)
    createActions[0]()
    expect(state.pushModal).toHaveBeenCalledExactlyOnceWith(FileIndexScopeDialog, expect.objectContaining({ volume: state.volumes[configured] }))
    expect(state.mutation).not.toHaveBeenCalled()
  } finally {
    state.initial = null
    state.volumes = []
  }
})
