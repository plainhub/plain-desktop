import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { useSafeMarkdown } from '@/hooks/markdown'

vi.mock('@/lib/api/file', () => ({ getFileUrlByPath: vi.fn() }))

it('renders document headings and tables without executing embedded HTML or script links', async () => {
  const { render } = useSafeMarkdown(ref({ appDir: '' }), ref(null))
  const html = await render('# Preview\n\n| Name | Value |\n| --- | --- |\n| One | Two |\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n[unsafe](javascript:alert(1))')
  expect(html).toContain('<h1')
  expect(html).toContain('<table>')
  expect(html).not.toContain('<script>')
  expect(html).not.toContain('<img ')
  expect(html).not.toContain('href="javascript:')
})
