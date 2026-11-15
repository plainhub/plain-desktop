import { expect, it } from 'vitest'
import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'
import FileIndexDefaults from '@/views/home/file-index/FileIndexDefaults.vue'

it.each([false, true])('renders host rules and profile exceptions from data (%s)', async (withProfiles) => {
  const app = createSSRApp(FileIndexDefaults, {
    rules: {
      excludedDirs: ['/system-data', '/cache-data'],
      excludedNames: ['node_modules', 'target'],
      mediaExcludedDirSuffixes: ['.bundle'],
      excludedProfileDirs: withProfiles ? [{
        profilesDir: '/profiles/', excludedDir: 'AppData', exceptDirs: ['AppData/Documents'],
      }] : [],
    },
  })
  app.config.globalProperties.$t = (key: string, values?: Record<string, string>) => [key, ...Object.values(values ?? {})].join(' ')
  const html = await renderToString(app)
  for (const text of ['/system-data', '/cache-data', 'node_modules', 'target', '.bundle', 'file_index.files_hint', 'file_index.media_hint']) {
    expect(html).toContain(text)
  }
  expect(html.includes('file_index.profile_hint')).toBe(withProfiles)
  expect(html.includes('/profiles/*/AppData')).toBe(withProfiles)
  expect(html.includes('/profiles/*/AppData/Documents')).toBe(withProfiles)
  expect(html).not.toContain('Library')
  expect(html).not.toContain('iCloud')
  expect(html).not.toContain('/Applications')
})
