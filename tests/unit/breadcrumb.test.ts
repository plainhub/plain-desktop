import { expect, it } from 'vitest'
import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import VBreadcrumb from '@/components/base/VBreadcrumb.vue'
import tooltip from '@/plugins/tooltip'

it('renders ancestors as links, marks only the current folder and retains full names for hover', async () => {
  const name = 'Very-long-folder-name-ending-photos'
  let bindings!: { nameParts(name: string): string[]; itemTooltip(item: { path: string; name: string }, index: number): string }
  const component = VBreadcrumb as any
  const items = [{ path: '/archive', name: 'Archive' }, { path: '/archive/photos', name }]
  const app = createSSRApp({ ...component, setup(props: unknown, context: unknown) {
    bindings = component.setup(props, context)
    return bindings
  } }, { items, label: 'Current path', wrap: true })
  app.use(tooltip)
  const html = await renderToString(app)
  expect(html.match(/<a\b/g)).toHaveLength(1)
  expect(html.match(/aria-current="page"/g)).toHaveLength(1)
  expect(bindings.nameParts(name)).toEqual(['Very-long-folder-name-ending-', 'photos'])
  expect(bindings.itemTooltip(items[1], 1)).toBe(name)
  const unicode = '年度工作资料与归档文件夹📁'
  expect(bindings.nameParts(unicode).join('')).toBe(unicode)
  expect(bindings.nameParts('Reports')).toEqual(['Reports', ''])
})
