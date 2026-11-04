interface InlineToken {
  type: string
  tag: string
  nesting: number
  markup: string
  content: string
}

interface Delimiter {
  marker: number
  length: number
  token: number
  end: number
  open: boolean
  close: boolean
}

interface InlineState {
  src: string
  pos: number
  posMax: number
  tokens: InlineToken[]
  tokens_meta: Array<{ delimiters: Delimiter[] } | null>
  delimiters: Delimiter[]
  push(type: string, tag: string, nesting: number): InlineToken
  scanDelims(position: number, splitWord: boolean): { length: number; can_open: boolean; can_close: boolean }
  md: { inline: { skipToken(state: InlineState): void }; utils: { isMdAsciiPunct(code: number): boolean } }
}

type InlineRule = (state: InlineState, silent: boolean) => boolean
interface InlineRuler {
  before(before: string, name: string, rule: InlineRule): void
  after(after: string, name: string, rule: InlineRule): void
}

export function markdownInlineFormats(md: {
  inline: { ruler: InlineRuler; ruler2: { before(before: string, name: string, rule: (state: InlineState) => void): void } }
}) {
  for (const [symbol, tag] of [['~', 'sub'], ['^', 'sup']] as const) {
    md.inline.ruler.after('emphasis', tag, (state, silent) => {
      const start = state.pos
      if (silent || state.src[start] !== symbol || start + 2 >= state.posMax) return false
      state.pos++
      while (state.pos < state.posMax && state.src[state.pos] !== symbol) state.md.inline.skipToken(state)
      const end = state.pos
      const content = state.src.slice(start + 1, end)
      if (end === state.posMax || !content || /(^|[^\\])(?:\\\\)*\s/.test(content)) {
        state.pos = start
        return false
      }
      state.push(`${tag}_open`, tag, 1).markup = symbol
      state.push('text', '', 0).content = content.replace(/\\(.)/g, (escape, character: string) =>
        character === ' ' || state.md.utils.isMdAsciiPunct(character.charCodeAt(0)) ? character : escape)
      state.push(`${tag}_close`, tag, -1).markup = symbol
      state.pos = end + 1
      return true
    })
  }

  for (const [symbol, tag] of [['+', 'ins'], ['=', 'mark']] as const) {
    const marker = symbol.charCodeAt(0)
    md.inline.ruler.before('emphasis', tag, (state, silent) => {
      if (silent || state.src[state.pos] !== symbol) return false
      const run = state.scanDelims(state.pos, true)
      if (run.length < 2) return false
      if (run.length % 2) state.push('text', '', 0).content = symbol
      for (let offset = run.length % 2; offset < run.length; offset += 2) {
        state.push('text', '', 0).content = symbol.repeat(2)
        if (run.can_open || run.can_close) state.delimiters.push({
          marker, length: 0, token: state.tokens.length - 1, end: -1, open: run.can_open, close: run.can_close,
        })
      }
      state.pos += run.length
      return true
    })
    md.inline.ruler2.before('emphasis', tag, (state) => {
      const lists = [state.delimiters, ...state.tokens_meta.flatMap((meta) => meta ? [meta.delimiters] : [])]
      for (const delimiters of lists) {
        const literals: number[] = []
        for (const delimiter of delimiters) {
          if (delimiter.marker !== marker || delimiter.end < 0) continue
          const end = delimiters[delimiter.end]!
          Object.assign(state.tokens[delimiter.token]!, { type: `${tag}_open`, tag, nesting: 1, markup: symbol.repeat(2), content: '' })
          Object.assign(state.tokens[end.token]!, { type: `${tag}_close`, tag, nesting: -1, markup: symbol.repeat(2), content: '' })
          const previous = state.tokens[end.token - 1]
          if (previous?.type === 'text' && previous.content === symbol) literals.push(end.token - 1)
        }
        for (const index of literals.reverse()) {
          let last = index + 1
          while (state.tokens[last]?.type === `${tag}_close`) last++
          last--
          const literal = state.tokens[index]!
          state.tokens[index] = state.tokens[last]!
          state.tokens[last] = literal
        }
      }
    })
  }
}
