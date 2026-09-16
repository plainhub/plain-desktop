import { computed, ref, watch } from 'vue'
import type { Ref } from 'vue'
import { collectJsonPathMatches, generateSuggestions, type PathSuggestion } from '@/lib/jsonpath'
import { formatJsonText, formatXmlText, minifyJsonText, parseJsonWithPosition, tryFixJson, xmlToTreeFromText } from './json-utils'
import { BIG_FILE_BYTES, cmLanguageFor, fileKindOf, isStructuredKind, type FileKind } from './file-kind'

export type ViewerMode = 'source' | 'split' | 'tree'

export function useViewer(opts: {
  fileName: Ref<string>
  content: Ref<string>
}) {
  const kind = computed(() => fileKindOf(opts.fileName.value))
  const structured = computed(() => isStructuredKind(kind.value))

  const sizeBytes = computed(() => {
    if (typeof TextEncoder === 'undefined') return opts.content.value.length
    return new TextEncoder().encode(opts.content.value).length
  })
  const big = computed(() => sizeBytes.value > BIG_FILE_BYTES)

  const jsonParse = computed(() => (kind.value === 'json' ? parseJsonWithPosition(opts.content.value) : null))
  const jsonValue = computed(() => jsonParse.value?.error ? undefined : jsonParse.value?.value)
  const jsonInvalid = computed(() => kind.value === 'json' && !!jsonParse.value?.error)
  const errorPos = computed(() => {
    if (!jsonInvalid.value || !jsonParse.value?.error) return null
    return { line: jsonParse.value.error.line, col: jsonParse.value.error.column }
  })

  const xmlTree = computed(() => (kind.value === 'xml' ? xmlToTreeFromText(opts.content.value) : null))
  const xmlValid = computed(() => kind.value !== 'xml' || xmlTree.value !== null)

  const mode = ref<ViewerMode>('split')
  const depth = ref(2)
  const pathOpen = ref(false)
  const expression = ref('')
  const wrap = ref(true)
  const indentSize = ref<2 | 4>(2)
  const folded = ref<'source' | 'tree' | null>(null)
  const ratio = ref(0.5)

  const viewText = ref('')
  function applyIndent() {
    if (!structured.value) return
    if (kind.value === 'json') {
      const formatted = formatJsonText(opts.content.value, indentSize.value)
      if (formatted !== null) viewText.value = formatted
    } else if (kind.value === 'xml' && xmlValid.value) {
      const formatted = formatXmlText(opts.content.value, indentSize.value)
      if (formatted !== null) viewText.value = formatted
    }
  }
  watch(
    () => opts.content.value,
    (val) => {
      viewText.value = val
      mode.value = structured.value ? 'split' : 'source'
      pathOpen.value = false
      expression.value = ''
      folded.value = null
    },
    { immediate: true },
  )
  watch(indentSize, applyIndent)

  const suggestions = computed<PathSuggestion[]>(() => {
    if (kind.value !== 'json' || jsonValue.value === undefined) return []
    if (!expression.value.trim() || expression.value === '$') return []
    try {
      return generateSuggestions(jsonValue.value, expression.value)
    } catch {
      return []
    }
  })

  const matchPaths = computed<Set<string> | null>(() => {
    if (kind.value !== 'json' || jsonValue.value === undefined) return null
    const expr = expression.value.trim()
    if (!pathOpen.value || !expr || expr === '$') return null
    try {
      return new Set(collectJsonPathMatches(jsonValue.value, expr))
    } catch {
      return null
    }
  })

  const matchCount = computed(() => {
    if (kind.value !== 'json') return null
    if (!pathOpen.value || !expression.value.trim()) return null
    if (jsonValue.value === undefined) return null
    const paths = matchPaths.value
    return paths ? paths.size : -1
  })

  const canTransform = computed(() => !big.value && structured.value && (kind.value === 'json' ? !jsonInvalid.value : xmlValid.value))

  const lines = computed(() => (viewText.value ? viewText.value.split('\n').length : 0))

  function format() {
    if (!canTransform.value) return
    applyIndent()
  }

  function minify() {
    if (!canTransform.value || kind.value !== 'json') return
    const minified = minifyJsonText(opts.content.value)
    if (minified !== null) viewText.value = minified
  }

  const fixable = computed(() => {
    if (kind.value !== 'json' || !jsonInvalid.value || big.value) return false
    return tryFixJson(opts.content.value) !== null
  })

  function fix() {
    if (!fixable.value) return
    const result = tryFixJson(opts.content.value)
    if (!result) return
    const formatted = formatJsonText(result.fixed)
    viewText.value = formatted ?? result.fixed
  }

  const cmLanguage = computed(() => cmLanguageFor(kind.value, big.value))

  return {
    kind, structured, big, sizeBytes,
    jsonValue, jsonInvalid, errorPos, fixable, xmlTree, xmlValid,
    mode, depth, pathOpen, expression, wrap, indentSize, folded, ratio,
    viewText, suggestions, matchPaths, matchCount, canTransform, lines,
    cmLanguage,
    format, minify, fix,
  }
}
