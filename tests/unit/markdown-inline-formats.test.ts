import { expect, it } from 'vitest'
import MarkdownIt from 'markdown-it'
import fixtures from '../fixtures/markdown-inline-formats.json'
import { markdownInlineFormats } from '../../src/lib/markdown-inline-formats'

const renderer = new MarkdownIt({ html: false, xhtmlOut: true, breaks: true, typographer: true, linkify: true })
  .use(markdownInlineFormats)

it.each(fixtures)('preserves inline formatting: $source', ({ source, html }) => {
  expect(renderer.render(source)).toBe(html)
})
