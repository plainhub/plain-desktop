import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import ViewerToolbar from '@/views/text-file/ViewerToolbar.vue'
import StatusBar from '@/views/text-file/StatusBar.vue'
import JsonPathBar from '@/views/text-file/JsonPathBar.vue'
import SplitPanels from '@/views/text-file/SplitPanels.vue'

function mountTui(
  component: Parameters<typeof mount>[0],
  props: Record<string, unknown>,
  options: { slots?: Record<string, string> } = {},
) {
  return mount(component, {
    props,
    slots: options.slots,
    global: { mocks: { $t: (key: string, params?: Record<string, unknown>) => key } },
  })
}

describe('ViewerToolbar', () => {
  it('shows mode seg, format, depth and jsonpath for json', () => {
    const wrapper = mountTui(ViewerToolbar, {
      kind: 'json', structured: true, mode: 'split', depth: 2,
      pathOpen: false, wrap: true, big: false, canTransform: true,
      fullscreen: false,
    })
    const text = wrapper.text()
    expect(text).toContain('viewer_source')
    expect(text).toContain('split_view')
    expect(text).toContain('viewer_tree')
    expect(text).toContain('format')
    expect(text).toContain('minify')
    expect(text).toContain('depth')
    expect(text).toContain('all')
    expect(text).toContain('JSONPath')
  })

  it('labels third mode as preview for markdown', () => {
    const wrapper = mountTui(ViewerToolbar, {
      kind: 'md', structured: false, mode: 'split', depth: 2,
      pathOpen: false, wrap: true, big: false, canTransform: false,
    })
    expect(wrapper.text()).toContain('preview')
    expect(wrapper.text()).not.toContain('JSONPath')
    expect(wrapper.text()).not.toContain('minify')
  })

  it('hides mode seg for txt and shows big chip when large', () => {
    const wrapper = mountTui(ViewerToolbar, {
      kind: 'txt', structured: false, mode: 'source', depth: 2,
      pathOpen: false, wrap: true, big: true, canTransform: false,
    })
    expect(wrapper.find('.mode-seg').exists()).toBe(false)
    expect(wrapper.text()).toContain('large_file_virtual')
    expect(wrapper.text()).toContain('wrap')
  })

  it('disables format when transforms unavailable', () => {
    const wrapper = mountTui(ViewerToolbar, {
      kind: 'json', structured: true, mode: 'split', depth: 2,
      pathOpen: false, wrap: true, big: true, canTransform: false,
    })
    const formatBtn = wrapper.findAll('button').find(b => b.text().includes('format'))
    expect(formatBtn?.attributes('disabled')).toBeDefined()
  })

  it('emits depth and mode updates', async () => {
    const wrapper = mountTui(ViewerToolbar, {
      kind: 'json', structured: true, mode: 'split', depth: 2,
      pathOpen: false, wrap: true, big: false, canTransform: true,
      fullscreen: false,
    })
    await wrapper.findAll('.seg button').find(b => b.text() === '4')!.trigger('click')
    expect(wrapper.emitted('update:depth')![0]).toEqual([4])
    await wrapper.findAll('.mode-seg button')[0].trigger('click')
    expect(wrapper.emitted('update:mode')![0]).toEqual(['source'])
  })

  it('hides depth in source mode and shows indent switch', () => {
    const wrapper = mountTui(ViewerToolbar, {
      kind: 'json', structured: true, mode: 'source', depth: 2,
      pathOpen: false, wrap: true, indentSize: 2, big: false, canTransform: true,
      fullscreen: false,
    })
    expect(wrapper.text()).not.toContain('depth')
    expect(wrapper.text()).not.toContain('all')
    const buttons = wrapper.findAll('button').map(b => b.text())
    expect(buttons).toContain('2')
    expect(buttons).toContain('4')
    expect(buttons).toContain('format')
    expect(buttons).toContain('wrap')
  })

  it('hides format and wrap in tree mode but keeps depth', () => {
    const wrapper = mountTui(ViewerToolbar, {
      kind: 'json', structured: true, mode: 'tree', depth: 2,
      pathOpen: false, wrap: true, indentSize: 2, big: false, canTransform: true,
      fullscreen: false,
    })
    const buttons = wrapper.findAll('button').map(b => b.text())
    expect(buttons).not.toContain('format')
    expect(buttons).not.toContain('minify')
    expect(buttons).not.toContain('wrap')
    expect(wrapper.text()).toContain('depth')
  })

  it('shows fullscreen toggle and emits toggle-fullscreen', async () => {
    const base = { kind: 'json', structured: true, mode: 'split', depth: 2, pathOpen: false, wrap: true, big: false, canTransform: true, fullscreen: false }
    const wrapper = mountTui(ViewerToolbar, { ...base })
    const btn = wrapper.find('[aria-label="fullscreen"]')
    expect(btn.exists()).toBe(true)
    await btn.trigger('click')
    expect(wrapper.emitted('toggle-fullscreen')).toHaveLength(1)
    const active = mountTui(ViewerToolbar, { ...base, fullscreen: true })
    expect(active.find('[aria-label="exit_fullscreen"]').exists()).toBe(true)
  })
})

describe('StatusBar', () => {
  it('shows valid json verdict and single size display with lines', () => {
    const wrapper = mountTui(StatusBar, {
      kind: 'json', jsonInvalid: false, errorPos: null, fixable: false, lines: 128, sizeText: '3.2 KB',
    })
    expect(wrapper.text()).toContain('json_valid')
    expect(wrapper.text()).toContain('n_lines')
    expect(wrapper.text()).toContain('3.2 KB')
    expect(wrapper.text()).toContain('UTF-8')
    expect(wrapper.findAll('.stat')).toHaveLength(3)
  })

  it('shows invalid verdict with position and fix chip', async () => {
    const wrapper = mountTui(StatusBar, {
      kind: 'json', jsonInvalid: true, errorPos: { line: 5, col: 3 }, fixable: true, lines: 9, sizeText: '120 B',
    })
    expect(wrapper.text()).toContain('json_invalid')
    expect(wrapper.text()).toContain('error_line_col')
    const fix = wrapper.find('.fix-chip')
    expect(fix.exists()).toBe(true)
    await fix.trigger('click')
    expect(wrapper.emitted('fix')).toHaveLength(1)
  })

  it('shows doc hint without line count', () => {
    const wrapper = mountTui(StatusBar, {
      kind: 'doc', jsonInvalid: false, errorPos: null, fixable: false, lines: 0, sizeText: '2.1 MB',
    })
    expect(wrapper.text()).toContain('doc_preview_hint')
    expect(wrapper.text()).not.toContain('n_lines')
    expect(wrapper.text()).toContain('2.1 MB')
  })
})

describe('JsonPathBar', () => {
  it('emits expression updates and close', async () => {
    const wrapper = mountTui(JsonPathBar, { expression: '', matchCount: null, suggestions: [] })
    await wrapper.find('input').setValue('$.a')
    expect(wrapper.emitted('update:expression')![0]).toEqual(['$.a'])
    await wrapper.find('.icon-btn').trigger('click')
    expect(wrapper.emitted('close')).toHaveLength(1)
  })

  it('shows match count and invalid expression state', () => {
    const ok = mountTui(JsonPathBar, { expression: '$..x', matchCount: 2, suggestions: [] })
    expect(ok.text()).toContain('n_matches')
    const bad = mountTui(JsonPathBar, { expression: '$.[', matchCount: -1, suggestions: [] })
    expect(bad.text()).toContain('invalid_json_expr')
  })

  it('renders suggestions and applies on click', async () => {
    const wrapper = mountTui(JsonPathBar, {
      expression: '$.',
      matchCount: null,
      suggestions: [{ path: '$.apps', type: 'array' }],
    })
    await wrapper.find('input').trigger('focus')
    const suggestion = wrapper.find('.suggestion')
    expect(suggestion.exists()).toBe(true)
    await suggestion.trigger('click')
    expect(wrapper.emitted('update:expression')!.at(-1)).toEqual(['$.apps'])
  })
})

describe('SplitPanels', () => {
  it('renders only the source pane when tree is folded', () => {
    const wrapper = mountTui(SplitPanels, {
      ratio: 0.5,
      folded: 'tree',
    }, {
      slots: {
        source: '<div class="src">S</div>',
        tree: '<div class="tre">T</div>',
      },
    })
    expect(wrapper.find('.source-pane').exists()).toBe(true)
    expect(wrapper.find('.tree-pane').exists()).toBe(false)
    expect(wrapper.find('.fold-wing').exists()).toBe(false)
    expect(wrapper.text()).toContain('S')
  })

  it('renders tree pane full width when source folded (tree mode)', () => {
    const wrapper = mountTui(SplitPanels, {
      ratio: 0.5,
      folded: 'source',
    }, {
      slots: {
        source: '<div class="src">S</div>',
        tree: '<div class="tre">T</div>',
      },
    })
    expect(wrapper.find('.source-pane').exists()).toBe(false)
    expect(wrapper.find('.tree-pane').exists()).toBe(true)
    expect(wrapper.find('.fold-wing').exists()).toBe(false)
    expect(wrapper.text()).toContain('T')
    expect(wrapper.text()).not.toContain('S')
  })

  it('renders both panes with resize handle when unfolded', () => {
    const wrapper = mountTui(SplitPanels, { ratio: 0.5, folded: null }, {
      slots: {
        source: '<div class="src">S</div>',
        tree: '<div class="tre">T</div>',
      },
    })
    expect(wrapper.find('.source-pane').exists()).toBe(true)
    expect(wrapper.find('.tree-pane').exists()).toBe(true)
    expect(wrapper.find('.split-handle').exists()).toBe(true)
    expect(wrapper.find('.fold-wing').exists()).toBe(false)
  })
})
