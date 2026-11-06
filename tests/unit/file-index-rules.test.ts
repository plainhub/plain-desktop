import { expect, it } from 'vitest'
import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import FileIndexDefaults from '@/views/home/file-index/FileIndexDefaults.vue'

it.each([false, true])('renders the selected host rules and its user-library exception (%s)', async (excludedUserLibraries) => {
  const app = createSSRApp(FileIndexDefaults, {
    rules: {
      excludedDirs: ['/system-data', '/cache-data'],
      excludedNames: ['node_modules', 'target'],
      excludedPackages: ['.bundle'],
      excludedUserLibraries,
    },
  })
  app.config.globalProperties.$t = (key: string) => key
  const html = await renderToString(app)
  for (const text of ['/system-data', '/cache-data', 'node_modules', 'target', '.bundle', 'file_index.files_hint', 'file_index.media_hint']) {
    expect(html).toContain(text)
  }
  expect(html.includes('file_index.library_hint')).toBe(excludedUserLibraries)
  expect(html).not.toContain('/Applications')
})
