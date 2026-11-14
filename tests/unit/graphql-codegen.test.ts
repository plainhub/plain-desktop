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
