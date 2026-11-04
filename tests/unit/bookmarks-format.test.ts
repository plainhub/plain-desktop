import { afterEach, expect, it, vi } from 'vitest'
import { importBookmarksHtml } from '@/lib/bookmarks-format'

afterEach(() => vi.unstubAllGlobals())

it.each([false, true])('forwards the original HTML to the shared importer only when a file is selected: %s', async (selected) => {
  const html = '<DL><DT><A HREF="https://example.com/">Original &amp; 中文</A></DL>'
  const input = { type: '', accept: '', onchange: null as any, click: vi.fn() }
  vi.stubGlobal('document', { createElement: () => input })
  const importContent = vi.fn(async (_content: string) => {})
  importBookmarksHtml(importContent)
  expect(input.type).toBe('file')
  expect(input.accept).toBe('.html,.htm')
  expect(input.click).toHaveBeenCalledOnce()
  await input.onchange({ target: { files: selected ? [{ text: async () => html }] : [] } })
  if (selected) expect(importContent).toHaveBeenCalledExactlyOnceWith(html)
  else expect(importContent).not.toHaveBeenCalled()
})
