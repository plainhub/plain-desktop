import { createMarkdown } from '@/lib/markdown-renderer'
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

function useMarkdownRenderer(app: Ref<{ appDir: string }>, urlTokenKey: Ref<Uint8Array | null>, html: boolean) {
  const md = createMarkdown(html, { throwOnError: false, errorColor: '#cc0000' }, highlightCode)

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

export function useMarkdown(app: Ref<{ appDir: string }>, urlTokenKey: Ref<Uint8Array | null>) {
  return useMarkdownRenderer(app, urlTokenKey, true)
}

export function useSafeMarkdown(app: Ref<{ appDir: string }>, urlTokenKey: Ref<Uint8Array | null>) {
  return useMarkdownRenderer(app, urlTokenKey, false)
}
