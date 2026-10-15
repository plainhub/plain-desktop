import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { homeStatsGQL, HOME_STATS_COUNT_FIELDS, type HomeStatKey } from '../../../src/lib/api/home-stats'
import { parseDocument, parseSchema } from '../../../src/lib/api/graphql/parser'
import { validateDocument } from '../../../src/lib/api/graphql/schema'

const schema = parseSchema(readFileSync(new URL('../../../../plain-core/plain-rs/testdata/public-schema.graphqls', import.meta.url), 'utf8'))

describe('capability-selected home counts', () => {
  it('requests exactly the selected fields and declares the media filter only when used', () => {
    const keys = Object.keys(HOME_STATS_COUNT_FIELDS) as HomeStatKey[]
    for (let mask = 0; mask < 1 << keys.length; mask++) {
      const selected = keys.filter((_, index) => mask & (1 << index))
      const query = homeStatsGQL(selected)
      const document = parseDocument(query)
      expect(validateDocument(document, schema, 'homeStats')).toEqual([])
      const operation = document.operations[0]
      const fields = operation.selections.filter((item) => item.kind === 'field').map((item) => item.name)
      expect(fields).toEqual([...selected.map((key) => HOME_STATS_COUNT_FIELDS[key].split('(')[0]), 'mounts'])
      expect(operation.variables.map((item) => item.name)).toEqual(selected.some((key) => ['audios', 'images', 'videos'].includes(key)) ? ['mediaQuery'] : [])
    }
  })
})
