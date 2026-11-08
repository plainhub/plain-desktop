// Adapted from markdown-it-texmath 1.0.0 (SOURCE.json).
import katex from 'katex'

function canOpen(source, position) {
  const previous = source.charCodeAt(position - 1)
  return previous !== 0x5c && !(previous >= 0x30 && previous <= 0x39)
}

function canClose(source, position) {
  const next = source.charCodeAt(position)
  return !(next >= 0x30 && next <= 0x39)
}

function inline(state, silent, name, marker, pattern) {
  const { src, pos } = state
  if (!src.startsWith(marker, pos) || !canOpen(src, pos)) return false
  pattern.lastIndex = pos
  const match = pattern.exec(src)
  if (!match || !canClose(src, pattern.lastIndex)) return false
  if (!silent) {
    const token = state.push(name, 'math', 0)
    token.content = match[1]
    token.markup = marker
  }
  state.pos = pattern.lastIndex
  return true
}

function block(state, startLine, endLine, silent, name, pattern) {
  const pos = state.bMarks[startLine] + state.tShift[startLine]
  if (!state.src.startsWith('$$', pos)) return false
  pattern.lastIndex = pos
  const match = pattern.exec(state.src)
  if (!match) return false
  if (silent) return true

  const end = pattern.lastIndex - 1
  let line = startLine
  for (; line < endLine; line++) {
    if (end >= state.bMarks[line] + state.tShift[line] && end <= state.eMarks[line]) break
  }

  const token = state.push(name, 'math', 0)
  token.tag = '$$'
  token.content = state.parentType === 'blockquote' ? match[1].replace(/(\n*?^(?:\s*>)+)/gm, '') : match[1]
  token.info = match.at(-1)
  token.map = [startLine, line + 1]
  state.line = line + 1
  return true
}

export function math(markdown, { throwOnError = false, errorColor } = {}) {
  const options = { throwOnError, errorColor, output: 'html', trust: false, displayMode: false }
  const escape = (text) => markdown.utils.escapeHtml(text).replace(/'/g, '&#039;')
  function render(content, displayMode) {
    options.displayMode = displayMode
    try {
      return katex.renderToString(content, options)
    } catch (error) {
      return escape(`${content}:${error.message}`)
    }
  }

  const inlineRules = [
    ['math_inline_double', '$$', /\${2}([^$]*?[^\\])\${2}/gy, true],
    ['math_inline', '$', /\$((?:[^\s\\])|(?:\S.*?[^\s\\]))\$/gy, false],
  ]
  for (const [name, marker, pattern, displayMode] of inlineRules) {
    markdown.inline.ruler.before('escape', name, (state, silent) => inline(state, silent, name, marker, pattern))
    markdown.renderer.rules[name] = (tokens, index) => {
      const formula = render(tokens[index].content, displayMode)
      return displayMode ? `<section><eqn>${formula}</eqn></section>` : `<eq>${formula}</eq>`
    }
  }

  const blockRules = [
    ['math_block_eqno', /\${2}([^$]*?[^\\])\${2}\s*?\(([^)\s]+?)\)/gmy, true],
    ['math_block', /\${2}([^$]*?[^\\])\${2}/gmy, false],
  ]
  for (const [name, pattern, numbered] of blockRules) {
    markdown.block.ruler.before('fence', name, (state, start, end, silent) => block(state, start, end, silent, name, pattern))
    markdown.renderer.rules[name] = (tokens, index) => {
      const { content, info } = tokens[index]
      const formula = render(content, true)
      return numbered
        ? `<section class="eqno"><eqn>${formula}</eqn><span>(${escape(info)})</span></section>`
        : `<section><eqn>${formula}</eqn></section>`
    }
  }
}
