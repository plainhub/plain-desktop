import { expect, it, vi } from 'vitest'
import { h } from 'vue'
import SessionCard from '@/views/settings/sessions/SessionCard.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import VTextButton from '@/components/base/VTextButton.vue'
import InlineDeleteConfirm from '@/components/base/InlineDeleteConfirm.vue'
import VCircularProgress from '@/components/base/VCircularProgress.vue'
import type { SessionRecord } from '@/views/settings/sessions/api'
import { clientTemplate, renderView, nodes, content, fire, flush } from '../helpers/vue-node-renderer'

vi.mock('@/lib/format', () => ({ formatDateTime: (value: string) => value }))
vi.mock('@/components/base/VDropdown.vue', () => ({ default: {
  props: { modelValue: Boolean, borderless: Boolean }, emits: ['update:modelValue'],
  setup(props: any, { emit, slots }: any) { return () => h('div', [h('div', { onClick: () => emit('update:modelValue', !props.modelValue) }, slots.trigger?.()), props.modelValue ? slots.default?.() : null]) },
} }))
clientTemplate(SessionCard, 'src/views/settings/sessions/SessionCard.vue')
clientTemplate(VIconButton, 'src/components/base/VIconButton.vue')
clientTemplate(VTextButton, 'src/components/base/VTextButton.vue')

it('opens the raw session data from the leading icon', async () => {
  const entry: SessionRecord = { clientId: 'fixture-client', clientName: 'Fixture browser', type: 'LOGIN', osName: 'Fixture OS', osVersion: '1', appName: 'Fixture', appVersion: '1', clientIp: '192.0.2.24', createdAt: '2026-01-01T00:00:00Z' }
  const view = renderView(SessionCard, { entry, busy: false }, { VIconButton, VTextButton, InlineDeleteConfirm, VCircularProgress, VCopyButton: { render: () => null } })
  try {
    expect(nodes(view.root).some(node => node.tag === 'pre')).toBe(false)
    const icon = nodes(view.root).find(node => node.tag === 'button' && node.props['aria-label'] === 'session_management.view_raw')!
    let stopped = false
    const event = { stopPropagation() { stopped = true }, preventDefault() {} }
    for (let target: typeof icon | undefined = icon; target && !stopped; target = target.parent) fire(target, 'onClick', event)
    await flush()
    const raw = nodes(view.root).find(node => node.tag === 'pre')!
    expect(JSON.parse(content(raw))).toEqual(entry)
  } finally { view.unmount() }
})
