import { readApiSchema } from '../../../plain-core/scripts/read-api-schema.mjs'
import { describe, expect, it, vi } from 'vitest'
import { Capability, DataType } from '../../src/lib/data'
import { hasMediaDelete, hasMediaMove, hasMediaTrash } from '../../src/lib/feature'
import { parseSchema } from '../../src/lib/api/graphql/parser'
import { getAvailableFeatures } from '../../src/views/app-rail/features'

describe('declared capability support', () => {
  it('keeps the client enum equal to the shared main schema', () => {
    const schema = parseSchema(readApiSchema())
    expect(Object.values(Capability).sort()).toEqual(schema.types.get('Capability')!.enumValues.sort())
  })

  it('requires each media action capability independently', () => {
    vi.stubGlobal('__IS_TAURI__', false)
    try {
      for (const [capability, supported] of [
        [Capability.MEDIA_DELETE, hasMediaDelete],
        [Capability.MEDIA_MOVE, hasMediaMove],
        [Capability.MEDIA_TRASH, hasMediaTrash],
      ] as const) {
        expect(supported(undefined, DataType.IMAGE)).toBe(false)
        expect(supported({ capabilities: [] }, DataType.IMAGE)).toBe(false)
        expect(supported({ capabilities: [capability] }, DataType.IMAGE)).toBe(true)
        expect(supported({ capabilities: Object.values(Capability).filter((value) => value !== capability) }, DataType.IMAGE)).toBe(false)
      }
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('makes Image Editor available without special capabilities and preserves other capability gates', () => {
    const featureIds = (capabilities: Capability[]) => getAvailableFeatures(capabilities).map(feature => feature.id)
    expect(featureIds([])).toContain('IMAGE_EDITOR')
    expect(featureIds([])).not.toContain('SMS')
    expect(featureIds([Capability.TELEPHONY])).toContain('SMS')
  })
})
