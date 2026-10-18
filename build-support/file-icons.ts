import { readdirSync, readFileSync } from 'node:fs'
import type { Plugin } from 'vite'

export function fileIconAssets(directory = new URL('../../plain-core/assets/ficons/', import.meta.url)): Plugin {
  const icons = new Map(readdirSync(directory).filter((name) => name.endsWith('.svg')).sort()
    .map((name) => [name, readFileSync(new URL(name, directory))]))
  if (!icons.has('default.svg') || !icons.has('folder.svg')) throw new Error('Core file icons are incomplete')
  return {
    name: 'core-file-icons',
    enforce: 'pre',
    resolveId(id) {
      if (id.startsWith('/ficons/') && icons.has(id.slice('/ficons/'.length))) return `\0core-icon:${id}`
    },
    load(id) {
      if (id.startsWith('\0core-icon:')) return `export default ${JSON.stringify(id.slice('\0core-icon:'.length))}`
    },
    generateBundle() {
      for (const [name, source] of icons) this.emitFile({ type: 'asset', fileName: `ficons/${name}`, source })
    },
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const url = new URL(request.url ?? '/', 'http://localhost')
        if (!url.pathname.startsWith('/ficons/')) return next()
        if (request.method !== 'GET' && request.method !== 'HEAD') { response.statusCode = 405; response.end(); return }
        const icon = icons.get(url.pathname.slice('/ficons/'.length))
        if (!icon) { response.statusCode = 404; response.end(); return }
        response.setHeader('Content-Type', 'image/svg+xml')
        response.end(request.method === 'HEAD' ? undefined : icon)
      })
    },
  }
}
