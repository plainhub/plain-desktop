import shortcodeData from './data/emoji-shortcodes-iamcal.json'

export interface EmojiSuggestion {
  emoji: string
  shortcode: string
}

export interface ActiveEmojiShortcode {
  start: number
  end: number
  query: string
}

export interface EmojiReplacement {
  value: string
  caret: number
}

const MAX_SHORTCODE_LENGTH = 64
const DEFAULT_SUGGESTION_LIMIT = 8
const POPULAR_SHORTCODES = ['thumbsup', 'heart', 'joy', 'smile', 'laughing', 'tada', 'fire', 'eyes']

const emojiByShortcode = new Map<string, string>()
const shortcodeEntries: EmojiSuggestion[] = []

function hexcodeToEmoji(hexcode: string): string {
  const codepoints = hexcode.split('-').map((value) => Number.parseInt(value, 16))
  let emoji = String.fromCodePoint(...codepoints)
  if (codepoints.length === 1 && /\p{Emoji}/u.test(emoji) && !/\p{Emoji_Presentation}/u.test(emoji)) {
    emoji += '\uFE0F'
  }
  return emoji
}

for (const [hexcode, value] of Object.entries(shortcodeData)) {
  const emoji = hexcodeToEmoji(hexcode)

  const shortcodes = Array.isArray(value) ? value : [value]
  for (const shortcode of shortcodes) {
    const normalized = shortcode.toLowerCase()
    if (emojiByShortcode.has(normalized)) continue
    emojiByShortcode.set(normalized, emoji)
    shortcodeEntries.push({ emoji, shortcode })
  }
}

const popularSuggestions = POPULAR_SHORTCODES.flatMap((shortcode) => {
  const emoji = emojiByShortcode.get(shortcode)
  return emoji ? [{ emoji, shortcode }] : []
})

function shortcodePattern(closingColon: boolean): RegExp {
  const suffix = closingColon ? ':$' : '$'
  return new RegExp(`(?:^|\\s):([a-z0-9_+\\-]{${closingColon ? 1 : 0},${MAX_SHORTCODE_LENGTH}})${suffix}`, 'i')
}

function shortcodeStart(match: RegExpExecArray): number {
  return match.index + match[0].indexOf(':')
}

export function findActiveEmojiShortcode(value: string, caret: number): ActiveEmojiShortcode | null {
  if (caret < 0 || caret > value.length) return null

  const match = shortcodePattern(false).exec(value.slice(0, caret))
  if (!match) return null

  return {
    start: shortcodeStart(match),
    end: caret,
    query: match[1].toLowerCase(),
  }
}

export function getEmojiSuggestions(query: string, limit = DEFAULT_SUGGESTION_LIMIT): EmojiSuggestion[] {
  if (limit <= 0) return []

  const normalized = query.toLowerCase()
  if (!normalized) return popularSuggestions.slice(0, limit)

  return shortcodeEntries
    .filter((item) => item.shortcode.toLowerCase().includes(normalized))
    .sort((a, b) => {
      const aName = a.shortcode.toLowerCase()
      const bName = b.shortcode.toLowerCase()
      const aScore = aName === normalized ? 0 : aName.startsWith(normalized) ? 1 : 2
      const bScore = bName === normalized ? 0 : bName.startsWith(normalized) ? 1 : 2
      return aScore - bScore || aName.length - bName.length || aName.localeCompare(bName)
    })
    .slice(0, limit)
}

export function applyEmojiSuggestion(
  value: string,
  active: ActiveEmojiShortcode,
  suggestion: EmojiSuggestion,
): EmojiReplacement {
  const nextValue = value.slice(0, active.start) + suggestion.emoji + value.slice(active.end)
  return {
    value: nextValue,
    caret: active.start + suggestion.emoji.length,
  }
}

export function replaceCompletedEmojiShortcode(value: string, caret: number): EmojiReplacement | null {
  if (caret < 0 || caret > value.length) return null

  const match = shortcodePattern(true).exec(value.slice(0, caret))
  if (!match) return null

  const emoji = emojiByShortcode.get(match[1].toLowerCase())
  if (!emoji) return null

  const start = shortcodeStart(match)
  return {
    value: value.slice(0, start) + emoji + value.slice(caret),
    caret: start + emoji.length,
  }
}
