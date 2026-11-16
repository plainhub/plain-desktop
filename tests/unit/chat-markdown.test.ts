import { expect, it } from 'vitest'
import { renderChatMarkdown } from '@/lib/chat/chat-markdown'

it('renders formatted captions without HTML, external images or unsafe destinations', () => {
  const html = renderChatMarkdown('**bold** [link](https://example.invalid) ![tracking](https://example.invalid/pixel.png) <img src="https://example.invalid/html.png"> [unsafe](javascript:alert%281%29) [local](file:///abc)')
  expect(html).toContain('<strong>bold</strong>')
  expect(html).toContain('href="https://example.invalid"')
  expect(html).not.toContain('<img')
  expect(html).not.toMatch(/href="(?:javascript|file):/)
  expect(html).not.toMatch(/<[^>]+\ssrc=/)
})
