import { expect, it, vi } from 'vitest'
import { h } from 'vue'
import FileIndexVolumeCard from '@/views/home/file-index/FileIndexVolumeCard.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import VTextButton from '@/components/base/VTextButton.vue'
import VCircularProgress from '@/components/base/VCircularProgress.vue'
import type { FileIndexVolume } from '@/lib/api/file-index'
import { clientTemplate, renderView, nodes, content, fire, flush } from '../helpers/vue-node-renderer'

vi.mock('@/lib/format', () => ({ formatFileSize: (value: number) => String(value), formatDateTime: (value: string) => value }))
vi.mock('@/components/base/VDropdown.vue', () => ({ default: {
  props: { modelValue: Boolean, borderless: Boolean }, emits: ['update:modelValue'],
  setup(props: any, { emit, slots }: any) { return () => h('div', [h('div', { onClick: () => emit('update:modelValue', !props.modelValue) }, slots.trigger?.()), props.modelValue ? slots.default?.() : null]) },
} }))
clientTemplate(FileIndexVolumeCard, 'src/views/home/file-index/FileIndexVolumeCard.vue')
clientTemplate(VIconButton, 'src/components/base/VIconButton.vue')
clientTemplate(VTextButton, 'src/components/base/VTextButton.vue')

it('opens the matching index volume from its icon and preserves settings and scan actions', async () => {
  const volume: FileIndexVolume = { id: 'fixture-volume', path: '/fixture', isAvailable: true, indexMode: 'BLOCKLIST', includedDirs: [], excludedDirs: ['/fixture/excluded'], lastScanAt: null, lastScanStatus: null, lastScanCount: null, lastScanTotal: null }
  const settings = vi.fn()
  const scan = vi.fn()
  const view = renderView(FileIndexVolumeCard, { volume, title: 'Fixture volume', scanDisabled: false, onSettings: settings, onScan: scan }, { VIconButton, VTextButton, VCircularProgress })
  try {
    expect(nodes(view.root).some(node => node.tag === 'pre')).toBe(false)
    const icon = nodes(view.root).find(node => node.tag === 'i-lucide:hard-drive')!
    let stopped = false
    const event = { stopPropagation() { stopped = true }, preventDefault() {} }
    for (let target: typeof icon | undefined = icon; target && !stopped; target = target.parent) fire(target, 'onClick', event)
    await flush()
    const raw = nodes(view.root).find(node => node.tag === 'pre')!
    expect(JSON.parse(content(raw))).toEqual(volume)
    expect(icon.parent!.tag).toBe('div')
    expect(icon.parent!.props.title).toBeUndefined()
    expect(settings).not.toHaveBeenCalled()
    expect(scan).not.toHaveBeenCalled()
    fire(icon.parent!, 'onClick', event)
    await flush()
    expect(nodes(view.root).some(node => node.tag === 'pre')).toBe(false)
    fire(nodes(view.root).find(node => node.tag === 'button' && node.props['aria-label'] === 'file_index.scope: Fixture volume')!, 'onClick', event)
    fire(nodes(view.root).find(node => node.tag === 'button' && content(node) === 'rebuild_index')!, 'onClick', event)
    expect(settings).toHaveBeenCalledOnce()
    expect(scan).toHaveBeenCalledOnce()
  } finally { view.unmount() }
})

it.each([
  [null, null, null, null, 'file_index.not_scanned'],
  ['2026-11-15T02:30:00Z', 'COMPLETED', 0, 0, 'file_index.scanned_files'],
  ['2026-11-15T02:30:00Z', 'CANCELED', 2, null, 'stopped'],
  ['2026-11-15T02:30:00Z', 'FAILED', 3, 5, 'failed'],
] as const)('renders the stored scan result with unknown and zero counts', (lastScanAt, lastScanStatus, lastScanCount, lastScanTotal, label) => {
  const volume: FileIndexVolume = { id: 'fixture-volume', path: '/fixture', isAvailable: true, indexMode: 'BLOCKLIST', includedDirs: [], excludedDirs: [], lastScanAt, lastScanStatus, lastScanCount, lastScanTotal }
  const view = renderView(FileIndexVolumeCard, { volume, title: 'Fixture volume', scanDisabled: false }, { VIconButton, VTextButton, VCircularProgress })
  try {
    expect(content(view.root)).toContain(label)
    if (lastScanAt) expect(content(view.root)).not.toContain(lastScanAt)
    expect(nodes(view.root).some(node => node.tag === 'pre')).toBe(false)
  } finally { view.unmount() }
})

it.each([
  ['QUEUED', 'waiting', ['pause', 'stop']],
  ['RUNNING', 'file_index.scanned_files', ['pause', 'stop']],
  ['PAUSED', 'paused', ['resume', 'stop']],
  ['FAILED', 'Generated read failure', []],
  ['STOPPED', 'stopped', []],
  ['IDLE', 'file_index.not_scanned', []],
] as const)('renders %s progress and only the matching volume controls', async (state, label, actions) => {
  const volume: FileIndexVolume = { id: 'fixture-volume', path: '/fixture', isAvailable: true, indexMode: 'BLOCKLIST', includedDirs: [], excludedDirs: [], lastScanAt: null, lastScanStatus: null, lastScanCount: null, lastScanTotal: null }
  const pause = vi.fn(), resume = vi.fn(), stop = vi.fn()
  const view = renderView(FileIndexVolumeCard, { volume, title: 'Fixture', scanDisabled: false,
    scanProgress: { volumeId: volume.id, state, indexed: 1234, total: null, pending: null, error: 'Generated read failure' },
    onPause: pause, onResume: resume, onStop: stop,
  }, { VIconButton, VTextButton, VCircularProgress })
  try {
    const status = nodes(view.root).find(node => node.props['aria-live'] === 'polite')
    if (label) expect(content(status!)).toContain(label)
    else expect(status).toBeUndefined()
    for (const [action, callback] of [['pause', pause], ['resume', resume], ['stop', stop]] as const) {
      const button = nodes(view.root).find(node => node.tag === 'button' && node.props['aria-label'] === action)
      expect(Boolean(button)).toBe(actions.includes(action as never))
      if (button) { fire(button, 'onClick', {}); expect(callback).toHaveBeenCalledOnce() }
    }
    expect(content(view.root).includes('stopped')).toBe(state === 'STOPPED')
    expect(content(view.root).includes('rebuild_index')).toBe(actions.length === 0)
  } finally { view.unmount() }
})

it.each([
  [0, null], [1234, null], [1234, 2000], [0, 2], [0, 0],
] as const)('renders committed progress %i with total %s without inventing totals', (indexed, total) => {
  const volume: FileIndexVolume = { id: 'fixture-volume', path: '/fixture', isAvailable: true, indexMode: 'BLOCKLIST', includedDirs: [], excludedDirs: [], lastScanAt: null, lastScanStatus: null, lastScanCount: null, lastScanTotal: null }
  const view = renderView(FileIndexVolumeCard, { volume, title: 'Fixture', scanDisabled: false,
    scanProgress: { volumeId: volume.id, state: 'RUNNING', indexed, total, pending: total === null ? null : total - indexed },
  }, { VIconButton, VTextButton, VCircularProgress })
  try {
    const text = content(nodes(view.root).find(node => node.props['aria-live'] === 'polite')!)
    expect(text.trim()).toBe(indexed > 0 ? 'file_index.scanned_files' : 'file_index.scanning')
  } finally { view.unmount() }
})

it('shows only current progress while scanning and file count with scope when complete', () => {
  const volume: FileIndexVolume = { id: 'fixture-volume', path: '/fixture', isAvailable: true, indexMode: 'BLOCKLIST', includedDirs: [], excludedDirs: [], lastScanAt: '2026-11-15T02:30:00Z', lastScanStatus: 'COMPLETED', lastScanCount: 584, lastScanTotal: 584 }
  const base = { volume, title: 'Fixture', scanDisabled: false }
  const components = { VIconButton, VTextButton, VCircularProgress }
  const running = renderView(FileIndexVolumeCard, { ...base, scanProgress: { volumeId: volume.id, state: 'RUNNING', indexed: 0, total: null, pending: null } }, components)
  const completed = renderView(FileIndexVolumeCard, base, components)
  try {
    const progress = content(running.root)
    expect(progress).toContain('file_index.scanning')
    expect(progress).not.toMatch(/file_index\.(scan_count|scanned_files|not_scanned|scope_all_folders)|2026-11-15/)
    expect(content(completed.root)).toContain('file_index.scanned_files')
    expect(content(completed.root)).toContain('file_index.scope_all_folders')
    expect(content(completed.root)).not.toContain('file_index.scope_excluded_folders')
  } finally { running.unmount(); completed.unmount() }
})
