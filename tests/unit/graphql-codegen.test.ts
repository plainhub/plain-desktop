import { expect, it } from 'vitest'
import { parseDocument, parseSchema } from '@/lib/api/graphql/parser'
import { generateOperationsSource } from '@/lib/api/graphql/codegen'

it('retains nullable and omitted rebuild scope in generated variable types', () => {
  const schema = parseSchema('type Mutation { rebuildFileIndex(volumeId: ID): Boolean! }')
  const ir = parseDocument('mutation($volumeId: ID) { rebuildFileIndex(volumeId: $volumeId) }')
  const source = generateOperationsSource([{ key: 'rebuildFileIndexGQL', ir }], schema)
  expect(source).toContain('volumeId?: string | null')
  expect(source).toContain('rebuildFileIndex: boolean')
})

it('keeps concrete chat content discriminants in union selections', () => {
  const schema = parseSchema(`
    input TextInput { text: String! retry: Int! = 0 }
    input FilesInput { ids: [ID!]! }
    input ChatContentInput @oneOf { text: TextInput files: FilesInput }
    type TextContent { text: String! }
    type FilesContent { count: Int! }
    union Content = TextContent | FilesContent
    type Mutation { send(content: ChatContentInput!): Content! }
  `)
  const ir = parseDocument('mutation($content:ChatContentInput!){send(content:$content){... on TextContent{__typename text} ... on FilesContent{__typename count}}}')
  const source = generateOperationsSource([{ key: 'send', ir }], schema)
  expect(source).toContain('__typename: "TextContent"')
  expect(source).toContain('__typename: "FilesContent"')
  expect(source).not.toContain('__typename?: unknown')
  expect(source).toContain('text: Input_TextInput')
  expect(source).toContain('files?: never')
  expect(source).toContain('text?: never')
  expect(source).toContain('retry?: number')
  expect(source).not.toContain('content: unknown')
})
