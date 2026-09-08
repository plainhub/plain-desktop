import { HighlightStyle, LanguageDescription, syntaxHighlighting } from '@codemirror/language'
import { languages } from '@codemirror/language-data'
import type { Parser } from '@lezer/common'
import { highlightTree, tagHighlighter, tags as t } from '@lezer/highlight'

const tokenGroups = [
  { class: 'md-tok-comment', tags: [t.comment, t.meta] },
  { class: 'md-tok-string', tags: [t.string, t.regexp, t.escape, t.character] },
  { class: 'md-tok-keyword', tags: [t.keyword, t.operatorKeyword, t.modifier] },
  {
    class: 'md-tok-function',
    tags: [t.function(t.variableName), t.function(t.propertyName), t.macroName],
  },
  {
    class: 'md-tok-constant',
    tags: [t.number, t.atom, t.color, t.constant(t.variableName)],
  },
  { class: 'md-tok-type', tags: [t.typeName, t.className, t.namespace, t.self] },
]

const tokenSpecs = tokenGroups.map(({ tags, class: cls }) => ({ tag: tags, class: cls }))

export const mdCodeHighlight = syntaxHighlighting(HighlightStyle.define(tokenSpecs))

const mdTagHighlighter = tagHighlighter(tokenSpecs)

const knownLanguages = new Map<string, { desc: LanguageDescription; parser?: Parser }>()

function languageFor(name: string): { desc: LanguageDescription; parser?: Parser } | null {
  const key = name.trim().toLowerCase()
  if (!key) return null
  let entry = knownLanguages.get(key)
  if (!entry) {
    const desc = LanguageDescription.matchLanguageName(languages, key)
    if (!desc) return null
    entry = { desc }
    knownLanguages.set(key, entry)
  }
  return entry
}

const fenceRe = /^[ \t]{0,3}(?:`{3,}|~{3,})[ \t]*([\w+#.-]+)/gm

export async function preloadCodeLanguages(source: string): Promise<void> {
  const names = new Set<string>()
  for (const match of source.matchAll(fenceRe)) names.add(match[1])
  await Promise.all(
    [...names].map(async (name) => {
      const entry = languageFor(name)
      if (!entry) return
      const support = await entry.desc.load()
      entry.parser = support.language.parser
    })
  )
}

const maxHighlightLength = 64 * 1024

function escapeHtml(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

export function highlightCode(code: string, lang: string): string {
  const entry = languageFor(lang)
  if (!entry?.parser || code.length > maxHighlightLength) return escapeHtml(code)
  const out: string[] = []
  let pos = 0
  highlightTree(entry.parser.parse(code), mdTagHighlighter, (from, to, cls) => {
    if (from > pos) out.push(escapeHtml(code.slice(pos, from)))
    out.push(`<span class="${cls}">${escapeHtml(code.slice(from, to))}</span>`)
    pos = to
  })
  out.push(escapeHtml(code.slice(pos)))
  return out.join('')
}
