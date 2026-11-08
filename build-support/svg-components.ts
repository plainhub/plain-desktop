import { readFileSync } from 'node:fs'
import { compileTemplate } from 'vue/compiler-sfc'
import type { Plugin } from 'vite'
import iconAssets from './icon-assets/icons.json'

const icons = new Map(Object.entries(iconAssets.icons))
export const iconComponents = Object.entries(iconAssets.components)

export function iconComponent(tag: string) {
  const match = /^i-(lucide|material-symbols)[:-]([a-z0-9-]+)$/.exec(tag)
  const name = match && `${match[1]}/${match[2]}`
  return name && icons.has(name) ? `~icons/${name}` : undefined
}
const iconPrefix = '\0plain-icon:'

export function svgComponents(): Plugin {
  return {
    name: 'plain-svg-components',
    enforce: 'pre',
    resolveId(id) {
      if (id.startsWith('~icons/')) {
        const name = id.slice('~icons/'.length)
        if (!icons.has(name)) this.error(`Unknown icon: ${name}`)
        return iconPrefix + name
      }
    },
    load(id) {
      const icon = id.startsWith(iconPrefix) ? id.slice(iconPrefix.length) : undefined
      if (!icon && !id.endsWith('.svg?component')) return
      const filename = icon ? icon.replace('/', '-') + '.vue' : id.slice(0, -'?component'.length)
      // Vue removes literal style tags from templates, including SVG animations.
      const source = icon ? icons.get(icon)! : readFileSync(filename, 'utf8')
        .replace(/<style>/g, '<component is="style">').replace(/<\/style>/g, '</component>')
      const { code, errors } = compileTemplate({
        source,
        filename,
        id: icon ?? id,
        transformAssetUrls: icon ? undefined : false,
      })
      if (errors.length) this.error(errors.map(String).join('\n'))
      if (icon) {
        const name = filename.slice(0, -'.vue'.length)
        return `import { markRaw } from 'vue'\n${code}\nexport default markRaw({ name: ${JSON.stringify(name)}, render })`
      }
      return `${code}\nexport default { render }`
    },
  }
}
