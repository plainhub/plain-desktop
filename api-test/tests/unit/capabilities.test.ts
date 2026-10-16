import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { Capability } from '../../../src/lib/data'
import { hasMediaDelete, hasMediaMove, hasMediaTrash, hasFileExport, hasFileReveal, hasFileClipboard } from '../../../src/lib/feature'
import { parseSchema } from '../../../src/lib/api/graphql/parser'

describe('declared capability support', () => {
  it('keeps the client enum equal to the shared main schema', () => {
    const schema = parseSchema(readFileSync(new URL('../../../../plain-core/plain-rs/testdata/public-schema.graphqls', import.meta.url), 'utf8'))
    expect(Object.values(Capability).sort()).toEqual(schema.types.get('Capability')!.enumValues.sort())
  })

  it('requires each file and media action capability independently', () => {
    for (const [capability, supported] of [
      [Capability.FILE_EXPORT, hasFileExport],
      [Capability.FILE_REVEAL, hasFileReveal],
      [Capability.FILE_CLIPBOARD, hasFileClipboard],
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

it('does not infer file action support from the OS, device label or text clipboard', () => {
  const supported = { capabilities: [Capability.FILE_EXPORT, Capability.FILE_REVEAL, Capability.FILE_CLIPBOARD], platform: 'ANDROID', deviceType: 'NAS' }
  const unsupported = { capabilities: [Capability.CLIPBOARD], platform: 'MACOS', deviceType: 'COMPUTER' }
  for (const supports of [hasFileExport, hasFileReveal, hasFileClipboard]) {
    expect(supports(supported)).toBe(true)
    expect(supports(unsupported)).toBe(false)
  }
})
