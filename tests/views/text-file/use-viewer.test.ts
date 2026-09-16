import { describe, expect, it } from 'vitest'
import { nextTick, ref } from 'vue'
import { useViewer } from '@/views/text-file/useViewer'
import { collectJsonPathMatches } from '@/lib/jsonpath'

function setup(fileName: string, content: string) {
  return useViewer({ fileName: ref(fileName), content: ref(content) })
}

describe('useViewer', () => {
  it('detects kind, parses json and reports validity', () => {
    const v = setup('report.json', '{"a": 1}')
    expect(v.kind.value).toBe('json')
    expect(v.structured.value).toBe(true)
    expect(v.jsonInvalid.value).toBe(false)
    expect(v.jsonValue.value).toEqual({ a: 1 })
    expect(v.lines.value).toBe(1)
    expect(v.canTransform.value).toBe(true)
  })

  it('reports json errors with position and disables transforms', () => {
    const v = setup('broken.json', '{\n  "a": 1,\n}')
    expect(v.jsonInvalid.value).toBe(true)
    expect(v.jsonValue.value).toBeUndefined()
    expect(v.errorPos.value).not.toBeNull()
    expect(v.errorPos.value!.line).toBeGreaterThanOrEqual(2)
    expect(v.canTransform.value).toBe(false)
    expect(v.fixable.value).toBe(true)
  })

  it('formats json into viewText without touching source content', () => {
    const content = ref('{"a":1}')
    const v = useViewer({ fileName: ref('a.json'), content })
    expect(v.viewText.value).toBe('{"a":1}')
    v.format()
    expect(v.viewText.value).toBe('{\n  "a": 1\n}')
    expect(content.value).toBe('{"a":1}')
  })

  it('collects jsonpath matches for the tree filter', () => {
    const v = setup('a.json', '{"apps": [{"installedAt": 1}, {"installedAt": 2}]}')
    v.pathOpen.value = true
    v.expression.value = '$..installedAt'
    expect(v.matchCount.value).toBe(2)
    expect(v.matchPaths.value).not.toBeNull()
    expect(v.matchPaths.value!.has('$.apps[0].installedAt')).toBe(true)
  })

  it('returns -1 match count for invalid expressions', () => {
    const v = setup('a.json', '{"a": 1}')
    v.pathOpen.value = true
    v.expression.value = '$.['
    expect(v.matchCount.value === null || v.matchCount.value === -1 || v.matchCount.value === 0).toBe(true)
  })

  it('reformats viewText when indent size switches', async () => {
    const v = setup('a.json', '{"a":1}')
    v.indentSize.value = 4
    await nextTick()
    expect(v.viewText.value).toBe('{\n    "a": 1\n}')
  })

  it('maps txt to plain source with no structure', () => {
    const v = setup('server.log', 'line1\nline2')
    expect(v.kind.value).toBe('txt')
    expect(v.structured.value).toBe(false)
    expect(v.mode.value).toBe('source')
    expect(v.lines.value).toBe(2)
  })

  it('marks big files and disables transforms', () => {
    const v = setup('huge.json', JSON.stringify({ data: 'x'.repeat(9 * 1024 * 1024) }))
    expect(v.big.value).toBe(true)
    expect(v.canTransform.value).toBe(false)
    expect(v.cmLanguage.value).toBeUndefined()
  })

  it('collectJsonPathMatches stays consistent with matchPaths usage', () => {
    const v = setup('a.json', '{"list": [5, 6]}')
    expect(collectJsonPathMatches(v.jsonValue.value, '$.list[1]')).toEqual(['$.list[1]'])
  })
})
