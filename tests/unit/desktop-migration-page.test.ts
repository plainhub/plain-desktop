import { readFileSync } from 'node:fs'
import { runInNewContext } from 'node:vm'
import { expect, it } from 'vitest'

it('shows the approved Chinese copy and retries on the same local page', () => {
  const html = readFileSync(new URL('../../public/desktop-migration.html', import.meta.url), 'utf8')
  const source = html.match(/<script>([^]*?)<\/script>/)?.[1]
  expect(source).toBeDefined()
  const documentElement = { lang: 'en', dataset: { state: 'running' } }
  let click!: () => void
  const elements = {
    'progress-title': { textContent: '' },
    'failure-title': { textContent: '' },
    retry: { textContent: '', addEventListener(name: string, handler: () => void) {
      expect(name).toBe('click')
      click = handler
    } },
  }
  const location = { search: '' }
  runInNewContext(source!, {
    navigator: { language: 'zh-CN' },
    document: {
      documentElement,
      getElementById: (id: keyof typeof elements) => elements[id],
    },
    location,
  })
  expect(elements['progress-title'].textContent).toBe('正在迁移聊天记录')
  expect(elements['failure-title'].textContent).toBe('迁移失败，旧数据未删除')
  expect(elements.retry.textContent).toBe('重试')
  documentElement.dataset.state = 'failed'
  click()
  expect(documentElement.dataset.state).toBe('running')
  expect(location.search).toBe('?retry=1')
})
