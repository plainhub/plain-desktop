import { beforeEach, expect, it, vi } from 'vitest'
import { createSSRApp, h, ref } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { createI18n } from 'vue-i18n'
import { parseDocument } from 'htmlparser2'

vi.mock('@/lib/api/file', () => ({
  getFileUrlByPath: vi.fn((_key: Uint8Array | null, path: string) => `/resource?path=${encodeURIComponent(path)}`),
}))

import { getFileUrlByPath } from '@/lib/api/file'
import { useMarkdown, useSafeMarkdown } from '@/hooks/markdown'

function elements(source: string) {
  const nodes = parseDocument(source).children
  const found: Array<{ name: string; attribs: Record<string, string> }> = []
  const visit = (node: (typeof nodes)[number]) => {
    if ('name' in node && 'attribs' in node) found.push(node)
    if ('children' in node) node.children.forEach(visit)
  }
  nodes.forEach(visit)
  return found
}

beforeEach(() => vi.clearAllMocks())

it('preserves heading anchors and resolves local Markdown resources with the current key', async () => {
  const key = ref<Uint8Array | null>(new Uint8Array([1, 2]))
  const renderer = useMarkdown(ref({ appDir: '/library' }), key)
  const html = await renderer.render('# Chapter\n\n# Chapter\n\n[Note](app://notes/one.md)\n\n![Image](fid:photo)')
  const tags = elements(html)
  expect(tags.filter((node) => node.name === 'h1').map((node) => node.attribs.id)).toEqual(['chapter', 'chapter-1'])
  expect(getFileUrlByPath).toHaveBeenCalledTimes(2)
  expect(getFileUrlByPath).toHaveBeenCalledWith(key.value, '/library/notes/one.md')
  expect(getFileUrlByPath).toHaveBeenCalledWith(key.value, 'fid:photo')
  expect(tags.find((node) => node.name === 'img')?.attribs.src).toBe('/resource?path=fid%3Aphoto')
})

it.each([
  { mode: 'trusted', factory: useMarkdown, allowsHtml: true },
  { mode: 'safe', factory: useSafeMarkdown, allowsHtml: false },
])('retains the configured raw HTML boundary for $mode Markdown', async ({ factory, allowsHtml }) => {
  const renderer = factory(ref({ appDir: '/library' }), ref(null))
  const html = await renderer.render('<span data-probe="yes">text</span>')
  expect(elements(html).some((node) => node.attribs['data-probe'] === 'yes')).toBe(allowsHtml)
  expect(html).toContain('text')
})

it('preserves inline and display math in safe Markdown', async () => {
  const renderer = useSafeMarkdown(ref({ appDir: '/library' }), ref(null))
  const html = await renderer.render('Inline $x^2$.\n\n$$\\frac{1}{2}$$')
  const classes = elements(html).flatMap((node) => (node.attribs.class ?? '').split(' '))
  expect(classes).toContain('katex')
  expect(classes).toContain('katex-display')
})

it('renders invalid formulas without throwing or dropping the remaining text', async () => {
  const renderer = useSafeMarkdown(ref({ appDir: '/library' }), ref(null))
  const html = await renderer.render('$\\unknownSecurityCommand{x}$\n\nStill readable.')
  expect(html).toContain('unknownSecurityCommand')
  expect(html).toContain('Still readable.')
})

it('preserves code highlighting without interpreting the code as HTML', async () => {
  const renderer = useSafeMarkdown(ref({ appDir: '/library' }), ref(null))
  const html = await renderer.render('```typescript\nconst text = "<script>probe()</script>"\n```')
  expect(html).toContain('md-tok-keyword')
  expect(elements(html).some((node) => node.name === 'script')).toBe(false)
})

it('rejects JavaScript links while preserving ordinary external links', async () => {
  const renderer = useSafeMarkdown(ref({ appDir: '/library' }), ref(null))
  const html = await renderer.render('[Blocked](javascript:probe())\n\n[Allowed](https://example.test/note)')
  const links = elements(html).filter((node) => node.name === 'a')
  expect(links.map((node) => node.attribs.href)).toEqual(['https://example.test/note'])
  expect(getFileUrlByPath).not.toHaveBeenCalled()
})

it('keeps math trust disabled when the options prototype is polluted', async () => {
  const renderer = useSafeMarkdown(ref({ appDir: '/library' }), ref(null))
  const original = Object.getOwnPropertyDescriptor(Object.prototype, 'trust')
  let html: string
  Object.defineProperty(Object.prototype, 'trust', { value: true, writable: true, configurable: true })
  try {
    html = await renderer.render('Before $\\href{javascript:dependencyProbe()}{Open}$ after')
  } finally {
    if (original) Object.defineProperty(Object.prototype, 'trust', original)
    else delete (Object.prototype as { trust?: unknown }).trust
  }
  expect(elements(html).some((node) => node.attribs.href?.startsWith('javascript:'))).toBe(false)
  expect(html).toContain('Before')
  expect(html).toContain('after')
})

it('rejects carriage returns in serialized Vue attribute names', async () => {
  const app = createSSRApp({
    render: () => h('div', { 'data-note': 'kept', 'data-probe\ronpointerenter': 'dependencyProbe()' }, 'Text'),
  })
  const html = await renderToString(app)
  const div = elements(html).find((node) => node.name === 'div')
  expect(div?.attribs['data-note']).toBe('kept')
  expect(div?.attribs).not.toHaveProperty('onpointerenter')
  expect(html).toContain('Text')
})

it('neutralizes iframe srcdoc in escaped translation HTML', () => {
  const i18n = createI18n({
    legacy: false,
    locale: 'en',
    escapeParameter: true,
    messages: { en: { message: 'Before <iframe srcdoc="{content}"></iframe> after' } },
  })
  const html = i18n.global.t('message', { content: '<img src=x onerror="dependencyProbe()">' })
  expect(elements(html).some((node) => node.name === 'iframe' && Object.hasOwn(node.attribs, 'srcdoc'))).toBe(false)
  expect(html).toContain('Before')
  expect(html).toContain('after')
})

it('preserves translated interpolation and locale fallback', () => {
  const i18n = createI18n({
    legacy: false,
    locale: 'fr',
    fallbackLocale: 'en',
    messages: { en: { shared: 'Count: {count}' }, fr: { title: 'Bonjour {name}' } },
  })
  expect(i18n.global.t('title', { name: 'Reader' })).toBe('Bonjour Reader')
  expect(i18n.global.t('shared', { count: 3 })).toBe('Count: 3')
})
