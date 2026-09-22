import { describe, expect, it } from 'vitest'
import { extractDocuments } from '../../../../src/lib/api/graphql/extract'

function filesOf(map: Record<string, string>): Map<string, string> {
  return new Map(Object.entries(map))
}

describe('extractDocuments', () => {
  it('extracts a document and resolves same-file fragment interpolation', () => {
    const result = extractDocuments(
      filesOf({
        'lib/api.ts': [
          'const frag = `',
          'fragment F on T {',
          '  a',
          '}',
          '`',
          '',
          'export const listGQL = `',
          '  query {',
          '    items {',
          '      ...F',
          '    }',
          '  }',
          '  ${frag}',
          '`',
        ].join('\n'),
      }),
    )
    expect(result.docs).toHaveLength(2)
    const list = result.docs.find((d) => d.key === 'listGQL')!
    expect(list.text).toContain('fragment F on T')
    expect(list.text).toContain('...F')
  })

  it('resolves fragments imported across files, including @ aliases and renames', () => {
    const result = extractDocuments(
      filesOf({
        'lib/api/fragments.ts': 'export const itemFragment = `\nfragment Item on T {\n  id\n}\n`',
        'lib/api/query.ts':
          "import { itemFragment as frag } from '@/lib/api/fragments'\nexport const q = `\n  query {\n    items {\n      ...Item\n    }\n  }\n  ${frag}\n`\n",
      }),
    )
    const q = result.docs.find((d) => d.key === 'q')!
    expect(q.text).toContain('fragment Item on T')
  })

  it('warns instead of erroring when a hole is computed at runtime', () => {
    const result = extractDocuments(
      filesOf({
        'lib/api/query.ts':
          'export function dyn(keys: string[]) {\n  const fields = keys.join("\\n")\n  return `query {\n    ${fields}\n  }\n`\n}',
      }),
    )
    expect(result.docs).toHaveLength(0)
    expect(result.warnings).toHaveLength(1)
    expect(result.warnings[0]).toMatchObject({ name: 'dyn' })
  })

  it('warns when an identifier hole has no template binding', () => {
    const result = extractDocuments(
      filesOf({
        'lib/api/query.ts': 'import { gone } from "./gone"\nexport const q = `\n  query {\n    a\n  }\n  ${gone}\n`\n',
      }),
    )
    expect(result.docs).toHaveLength(0)
    expect(result.warnings).toHaveLength(1)
  })

  it('ignores plain strings and templates that are not documents', () => {
    const result = extractDocuments(
      filesOf({
        'lib/misc.ts': [
          "const notDoc = 'query syntax help for users'",
          'const tpl = `',
          '  query about things',
          '`',
          "const url = 'http://x/?query=1'",
        ].join('\n'),
      }),
    )
    expect(result.docs).toHaveLength(0)
    expect(result.warnings).toHaveLength(0)
  })

  it('does not treat backticks inside line comments or strings as templates', () => {
    const result = extractDocuments(
      filesOf({
        'lib/c.ts': "// don`t start a template here\nconst s = 'a ` b'\nconst doc = `query { x }`\n",
      }),
    )
    expect(result.docs.map((d) => d.key)).toEqual(['doc'])
  })
})
