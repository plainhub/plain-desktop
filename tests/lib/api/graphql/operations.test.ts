import { readFileSync, writeFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { generateOperationsArtifact, OPERATIONS_ARTIFACT } from './pipeline'

const writeMode = process.env.npm_lifecycle_event === 'graphql:codegen'

describe('graphql operations artifact', () => {
  it(writeMode ? 'regenerates operations.ts' : 'operations.ts matches the schemas and document corpus', () => {
    const generated = generateOperationsArtifact()
    if (writeMode) {
      writeFileSync(OPERATIONS_ARTIFACT, generated)
    }
    const committed = readFileSync(OPERATIONS_ARTIFACT, 'utf8')
    expect(committed).toBe(generated)
  })
})
