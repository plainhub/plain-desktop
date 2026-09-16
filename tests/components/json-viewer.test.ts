import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import JsonViewer from '@/components/jsonviewer/json-viewer.vue'

function mountViewer(value: unknown, expandDepth = 2) {
  return mount(JsonViewer, {
    props: { value, expandDepth },
    global: {
      mocks: { $t: (key: string) => key },
    },
  })
}

describe('JsonViewer', () => {
  it('renders expanded containers and value rows', () => {
    const wrapper = mountViewer({ name: 'plain', meta: { ver: '1.0' } }, 2)
    const text = wrapper.text()
    expect(text).toContain('"name"')
    expect(text).toContain('"plain"')
    expect(text).toContain('"ver"')
    expect(text).toContain('"1.0"')
    expect(text).toContain('{')
    expect(text).toContain('}')
  })

  it('collapses containers at expandDepth and toggles on click', async () => {
    const wrapper = mountViewer({ meta: { sub: { ver: '1.0' } } }, 2)
    expect(wrapper.text()).not.toContain('"ver"')
    const collapsedRows = wrapper.findAll('.tree-row--open')
    const subRow = collapsedRows.find(r => r.text().includes('"sub"'))
    expect(subRow).toBeTruthy()
    expect(subRow!.text()).toContain('… 1')
    await subRow!.trigger('click')
    expect(wrapper.text()).toContain('"ver"')
    expect(wrapper.text()).toContain('"1.0"')
  })

  it('shows comma separators between sibling entries', () => {
    const wrapper = mountViewer({ a: 1, b: 2 }, 2)
    const commas = wrapper.findAll('.tree-row .jv-punct').filter(n => n.text() === ',')
    expect(commas).toHaveLength(1)
  })

  it('renders empty state for null value', () => {
    const wrapper = mountViewer(null)
    expect(wrapper.find('.json-tree-empty').text()).toBe('no_data')
    expect(wrapper.findAll('.tree-row')).toHaveLength(0)
  })

  it('rebuilds rows when value changes', async () => {
    const wrapper = mountViewer({ a: 1 })
    expect(wrapper.text()).toContain('"a"')
    await wrapper.setProps({ value: { b: 2 } })
    expect(wrapper.text()).not.toContain('"a"')
    expect(wrapper.text()).toContain('"b"')
  })

  it('renders xml tags and attribute rows in xml mode', () => {
    const wrapper = mount(JsonViewer, {
      props: {
        value: { '@id': 'a1', item: { '@name': 'x' } },
        expandDepth: 2,
        xml: true,
        xmlRoot: 'root',
      },
      global: { mocks: { $t: (key: string) => key } },
    })
    const text = wrapper.text()
    expect(text).toContain('<root>')
    expect(text).toContain('</root>')
    expect(text).toContain('<item>')
    expect(text).toContain('</item>')
    expect(text).toContain('@id')
    expect(text).toContain('"a1"')
    expect(text).toContain('"x"')
    expect(text).not.toContain('"item"')
  })

  it('keeps only matched paths when filterPaths is set', () => {
    const wrapper = mountViewer({ keep: { hit: 1 }, drop: { x: 2 } }, 1)
    expect(wrapper.text()).toContain('"drop"')
    const filtered = mount(JsonViewer, {
      props: {
        value: { keep: { hit: 1 }, drop: { x: 2 } },
        expandDepth: 1,
        filterPaths: new Set(['$.keep.hit']),
      },
      global: { mocks: { $t: (key: string) => key } },
    })
    expect(filtered.text()).toContain('"hit"')
    expect(filtered.text()).not.toContain('"drop"')
  })
})
