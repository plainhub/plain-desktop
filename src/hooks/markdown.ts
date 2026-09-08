import 'katex/dist/katex.min.css'
import MarkdownIt from 'markdown-it'
import subscript from 'markdown-it-sub'
import superscript from 'markdown-it-sup'
import footnote from 'markdown-it-footnote'
import deflist from 'markdown-it-deflist'
import abbreviation from 'markdown-it-abbr'
import insert from 'markdown-it-ins'
import mark from 'markdown-it-mark'
import texmath from 'markdown-it-texmath'
import katex from 'katex'
import tasklists from 'markdown-it-task-lists'
import type { Ref } from 'vue'
import { parseDocument } from 'htmlparser2'
import { getFileUrlByPath } from '@/lib/api/file'
import { slugifyHeading } from '@/lib/md-editor'
import { highlightCode, preloadCodeLanguages } from '@/lib/md-code-highlight'

const VOID_TAGS = new Set(['area','base','br','col','embed','hr','img','input','link','meta','param','source','track','wbr'])

function escapeAttr(v: string): string {
  return v.replace(/&/g, '&amp;').replace(/"/g, '&quot;')
}

function renderNode(node: any): string {
  if (node.type === 'text') return (node.data ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  if (node.type === 'comment') return `<!--${(node.data ?? '').replace(/-->/g, '--\u003e')}-->`
  if (!node.name) return renderNodes(node.children ?? [])
  const safeName = String(node.name).replace(/[^a-zA-Z0-9-]/g, '')
  const attrs = node.attribs ? Object.entries(node.attribs).map(([k, v]) => ` ${k}="${escapeAttr(String(v))}"`).join('') : ''
  if (VOID_TAGS.has(safeName)) return `<${safeName}${attrs}>`
  return `<${safeName}${attrs}>${renderNodes(node.children ?? [])}</${safeName}>`
}

function renderNodes(nodes: any[]): string {
  return nodes.map(renderNode).join('')
}

function textContentOf(node: any): string {
  if (node.type === 'text') return node.data ?? ''
  return (node.children ?? []).map(textContentOf).join('')
}

// Assign GitHub-style anchor ids to headings so [toc](#slug) links can navigate.
function assignHeadingIds(nodes: Array<any>) {
  const seen = new Map<string, number>()
  const walk = (nodes: Array<any>) => {
    for (const node of nodes) {
      if (/^h[1-6]$/.test(node.name ?? '')) {
        const base = slugifyHeading(textContentOf(node)) || 'heading'
        const count = seen.get(base) ?? 0
        seen.set(base, count + 1)
        node.attribs = node.attribs ?? {}
        node.attribs.id = count === 0 ? base : `${base}-${count}`
      }
      walk(node.children ?? [])
    }
  }
  walk(nodes)
}

async function replaceNodes(nodes: Array<any>, replace: (link: string) => string | Promise<string>) {
  if (!nodes) return
  for (const node of nodes) {
    if (node.attribs) {
      if (node.name === 'img' && node.attribs.src) node.attribs.src = await replace(node.attribs.src)
      if (node.name === 'a' && node.attribs.href) node.attribs.href = await replace(node.attribs.href)
    }
    await replaceNodes(node.children ?? [], replace)
  }
}

export const useMarkdown = (app: Ref<{ appDir: string }>, urlTokenKey: Ref<Uint8Array | null>) => {
  const md = new MarkdownIt()
    .use(subscript).use(superscript).use(footnote).use(deflist)
    .use(abbreviation).use(insert).use(mark)
    .use(texmath, { engine: katex as any, delimiters: 'dollars', katexOptions: { throwOnError: false, output: 'html', errorColor: '#cc0000' } })
    .use(tasklists, { enabled: true })
  md.set({ html: true, xhtmlOut: true, breaks: true, linkify: true, typographer: true, highlight: highlightCode })

  const replace = (link: string) => {
    if (link.startsWith('app://')) {
      return getFileUrlByPath(urlTokenKey.value, app.value.appDir + '/' + link.replace('app://', ''))
    }
    if (link.startsWith('fid:')) {
      return getFileUrlByPath(urlTokenKey.value, link)
    }
    return link
  }

  return {
    render: async (source: string) => {
      await preloadCodeLanguages(source)
      const dom = parseDocument(md.render(source), { recognizeCDATA: true, recognizeSelfClosing: true })
      assignHeadingIds(dom.children)
      await replaceNodes(dom.children, replace)
      return renderNodes(dom.children)
    },
  }
}

/**
 * Safe variant for untrusted external content (e.g. RSS feed entries).
 * Raw HTML pass-through is disabled (html: false) to prevent XSS.
 * All other markdown features are enabled.
 */
export const useSafeMarkdown = (app: Ref<{ appDir: string }>, urlTokenKey: Ref<Uint8Array | null>) => {
  const md = new MarkdownIt()
    .use(subscript).use(superscript).use(footnote).use(deflist)
    .use(abbreviation).use(insert).use(mark)
    .use(texmath, { engine: katex as any, delimiters: 'dollars', katexOptions: { throwOnError: false, output: 'html', errorColor: '#cc0000' } })
    .use(tasklists, { enabled: true })
  // html: false — strips raw HTML from external feed content, blocking XSS
  md.set({ html: false, xhtmlOut: true, breaks: true, linkify: true, typographer: true, highlight: highlightCode })

  const replace = (link: string) => {
    if (link.startsWith('app://')) {
      return getFileUrlByPath(urlTokenKey.value, app.value.appDir + '/' + link.replace('app://', ''))
    }
    if (link.startsWith('fid:')) {
      return getFileUrlByPath(urlTokenKey.value, link)
    }
    return link
  }

  return {
    render: async (source: string) => {
      await preloadCodeLanguages(source)
      const dom = parseDocument(md.render(source), { recognizeCDATA: true, recognizeSelfClosing: true })
      assignHeadingIds(dom.children)
      await replaceNodes(dom.children, replace)
      return renderNodes(dom.children)
    },
  }
}

