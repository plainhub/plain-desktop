import { createMarkdown } from '@/lib/markdown-renderer'

const markdown = createMarkdown(false, { throwOnError: false }).disable('image')
// User messages may link out, but do not load remote resources or open local/system URLs.
const safeLink = markdown.validateLink.bind(markdown)
markdown.validateLink = (url: string) => /^(?:https?:\/\/|mailto:|#)/i.test(url) && safeLink(url)

export function renderChatMarkdown(text: string): string {
  return markdown.render(text)
}
