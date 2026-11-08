import 'katex/dist/katex.min.css'
import MarkdownIt from 'markdown-it'
import { abbreviations, definitionLists, footnotes, math } from './markdown-extensions/index.mjs'
import { markdownInlineFormats } from './markdown-inline-formats'
import { markdownTaskLists } from './markdown-task-lists'

export function createMarkdown(
  html: boolean,
  katexOptions: { throwOnError?: boolean; errorColor?: string } = {},
  highlight?: (code: string, language: string) => string,
) {
  return new MarkdownIt({ html, xhtmlOut: true, breaks: true, linkify: true, typographer: true, highlight })
    .use(markdownInlineFormats)
    .use(footnotes)
    .use(definitionLists)
    .use(abbreviations)
    .use(math, katexOptions)
    .use(markdownTaskLists)
}
