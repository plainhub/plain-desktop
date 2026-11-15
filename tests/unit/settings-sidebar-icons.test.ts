import { readFileSync } from 'node:fs'
import { expect, it } from 'vitest'
import { iconComponent } from '../../build-support/svg-components'

it('resolves every settings sidebar icon to a bundled SVG component', () => {
  const source = readFileSync(new URL('../../src/views/settings/SettingsSidebar.vue', import.meta.url), 'utf8')
  const tags = [...source.matchAll(/<(i-(?:lucide|material-symbols)[:-][a-z0-9-]+)/g)].map(match => match[1])
  expect(tags.length).toBeGreaterThan(0)
  expect(tags.filter(tag => !iconComponent(tag))).toEqual([])
})
