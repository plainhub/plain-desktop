import { expect, it } from 'vitest'
import MarkdownIt from 'markdown-it'
import { DomUtils, parseDocument } from 'htmlparser2'
import fixtures from '../fixtures/markdown-task-lists.json'
import { markdownTaskLists } from '../../src/lib/markdown-task-lists'

it.each(fixtures)('preserves task-list content and checkbox behavior: $source, html=$html', ({ source, html, expected }) => {
  const renderer = new MarkdownIt({ html }).use(markdownTaskLists)
  expect(DomUtils.getOuterHTML(parseDocument(renderer.render(source)))).toBe(expected)
})
