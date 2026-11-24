import { readApiSchema } from '../../../plain-core/scripts/read-api-schema.mjs'
import { describe, expect, it } from 'vitest'
import { Capability } from '../../src/lib/data'
import { hasMediaMove } from '../../src/lib/feature'
import { parseSchema } from '../../src/lib/api/graphql/parser'
import { getAvailableFeatures } from '../../src/views/app-rail/features'

describe('declared capability support', () => {
  it('keeps the client enum equal to the shared main schema', () => {
    const schema = parseSchema(readApiSchema())
    expect(Object.values(Capability).sort()).toEqual(schema.types.get('Capability')!.enumValues.sort())
  })

  it('only offers media move when the device declares support', () => {
    expect(hasMediaMove(undefined)).toBe(false)
    expect(hasMediaMove({ capabilities: [] })).toBe(false)
    expect(hasMediaMove({ capabilities: [Capability.MEDIA_MOVE] })).toBe(true)
  })

  it('makes Image Editor available without special capabilities and preserves other capability gates', () => {
    const featureIds = (capabilities: Capability[]) => getAvailableFeatures(capabilities).map(feature => feature.id)
    expect(featureIds([])).toContain('IMAGE_EDITOR')
    expect(featureIds([])).not.toContain('SMS')
    expect(featureIds([Capability.TELEPHONY])).toContain('SMS')
  })
})
