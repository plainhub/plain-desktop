import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { Capability } from '../../../src/lib/data'
import { hasMediaDelete, hasMediaMove, hasMediaTrash } from '../../../src/lib/feature'
import { parseSchema } from '../../../src/lib/api/graphql/parser'

describe('declared capability support', () => {
  it('keeps the client enum equal to the shared main schema', () => {
    const schema = parseSchema(readFileSync(new URL('../../../../plain-core/plain-rs/testdata/public-schema.graphqls', import.meta.url), 'utf8'))
    expect(Object.values(Capability).sort()).toEqual(schema.types.get('Capability')!.enumValues.sort())
  })

  it('requires each media action capability independently', () => {
    for (const [capability, supported] of [
      [Capability.MEDIA_DELETE, hasMediaDelete],
      [Capability.MEDIA_MOVE, hasMediaMove],
      [Capability.MEDIA_TRASH, hasMediaTrash],
    ] as const) {
      expect(supported(undefined)).toBe(false)
      expect(supported({ capabilities: [] })).toBe(false)
      expect(supported({ capabilities: [capability] })).toBe(true)
      expect(supported({ capabilities: Object.values(Capability).filter((value) => value !== capability) })).toBe(false)
    }
  })
})
