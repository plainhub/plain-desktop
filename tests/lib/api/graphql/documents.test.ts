import { describe, expect, it } from 'vitest'
import { loadCorpus, loadUnionSchema, validateCorpus } from './pipeline'

/** Allowlist of dynamic document builders — templates whose `${…}` holes
 *  are computed at runtime instead of interpolating fragment consts. Every
 *  entry here MUST also be exercised by `dynamicBuilderDocs` in pipeline.ts. */
const DYNAMIC_BUILDERS = ['homeStatsGQL']

describe('graphql document contract', () => {
  it('the local and extension schemas merge without overlap', () => {
    const { mergeErrors } = loadUnionSchema()
    expect(mergeErrors).toEqual([])
  })

  it('every scanned template with unresolvable holes is a registered builder', () => {
    const { extraction } = loadCorpus()
    const unregistered = extraction.warnings.filter((w) => !DYNAMIC_BUILDERS.includes(w.name))
    expect(unregistered).toEqual([])
    expect(extraction.errors).toEqual([])
  })

  it('every frontend document validates against the union schema', () => {
    const { schema } = loadUnionSchema()
    const corpus = loadCorpus()
    expect(corpus.documents.length).toBeGreaterThan(200)
    expect(validateCorpus(corpus, schema)).toEqual([])
  })

  it('catches a document selecting a field the schema does not define', () => {
    const { schema } = loadUnionSchema()
    const corpus = loadCorpus()
    const mutated = corpus.parsed.map((doc) =>
      doc.key === 'peersGQL'
        ? { ...doc, ir: { ...doc.ir, operations: doc.ir.operations.map((op) => ({ ...op, selections: [...op.selections, { kind: 'field', alias: undefined, name: 'no_such_field', args: [], selections: [] }] })) } }
        : doc,
    )
    const errors = validateCorpus({ ...corpus, parsed: mutated }, schema)
    expect(errors.some((e) => e.message.includes("unknown field 'no_such_field'"))).toBe(true)
  })
})
