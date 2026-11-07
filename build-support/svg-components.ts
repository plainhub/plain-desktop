import { readFileSync } from 'node:fs'
import { compileTemplate } from 'vue/compiler-sfc'
import type { Plugin } from 'vite'

export function svgComponents(): Plugin {
  return {
    name: 'plain-svg-components',
    enforce: 'pre',
    load(id) {
      if (!id.endsWith('.svg?component')) return
      const filename = id.slice(0, -'?component'.length)
      // Vue removes literal style tags from templates, including SVG animations.
      const source = readFileSync(filename, 'utf8').replace(/<style>/g, '<component is="style">').replace(/<\/style>/g, '</component>')
      const { code, errors } = compileTemplate({ source, filename, id, transformAssetUrls: false })
      if (errors.length) this.error(errors.map(String).join('\n'))
      return `${code}\nexport default { render }`
    },
  }
}
