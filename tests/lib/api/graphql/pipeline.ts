import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { extractDocuments, type ExtractedDoc, type ExtractResult } from '../../../../src/lib/api/graphql/extract'
import { parseDocument, parseSchema, type DocumentIR, type SchemaIR } from '../../../../src/lib/api/graphql/parser'
import { mergeSchemas, validateDocument, type ValidationError } from '../../../../src/lib/api/graphql/schema'
import { generateOperationsSource } from '../../../../src/lib/api/graphql/codegen'
import { homeStatsGQL } from '../../../../src/lib/api/query'

export const ROOT = process.cwd()
const SRC_DIR = join(ROOT, 'src')
const GRAPHQL_DIR = join(SRC_DIR, 'lib/api/graphql')

export function walkSrc(dir: string = SRC_DIR, out: Map<string, string> = new Map()): Map<string, string> {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry)
    if (statSync(full).isDirectory()) {
      if (entry === 'node_modules' || full === GRAPHQL_DIR) continue
      walkSrc(full, out)
    } else if (/\.(ts|vue)$/.test(entry)) {
      out.set(relative(SRC_DIR, full), readFileSync(full, 'utf8'))
    }
  }
  return out
}

export function loadUnionSchema(): { schema: SchemaIR; mergeErrors: ValidationError[] } {
  const local = parseSchema(readFileSync(join(GRAPHQL_DIR, 'local-schema.graphql'), 'utf8'))
  const extension = parseSchema(readFileSync(join(GRAPHQL_DIR, 'extension-schema.graphql'), 'utf8'))
  const merged = mergeSchemas(local, extension)
  return { schema: merged.schema, mergeErrors: merged.errors }
}

/** Dynamic document builders the static scanner cannot interpolate. Each
 *  entry must run with arguments covering every field any caller selects —
 *  a subset document's fields are always a subset of the full one. */
const ALL_HOME_STAT_KEYS = [
  'audios',
  'images',
  'videos',
  'docs',
  'packages',
  'notes',
  'feedEntries',
  'messages',
  'calls',
  'contacts',
] as const

export const dynamicBuilderDocs: ExtractedDoc[] = [
  { key: 'homeStatsGQL', file: 'lib/api/query.ts', text: homeStatsGQL([...ALL_HOME_STAT_KEYS]) },
]

export interface Corpus {
  extraction: ExtractResult
  documents: ExtractedDoc[]
  parsed: Array<{ key: string; file: string; ir: DocumentIR }>
}

export function loadCorpus(): Corpus {
  const extraction = extractDocuments(walkSrc())
  const documents = [...extraction.docs, ...dynamicBuilderDocs]
  const parsed = documents.map((doc) => ({ key: doc.key, file: doc.file, ir: parseDocument(doc.text) }))
  return { extraction, documents, parsed }
}

export function validateCorpus(corpus: Corpus, schema: SchemaIR): ValidationError[] {
  const errors: ValidationError[] = []
  for (const doc of corpus.parsed) {
    errors.push(...validateDocument(doc.ir, schema, doc.key))
  }
  return errors
}

export function generateOperationsArtifact(): string {
  const { schema, mergeErrors } = loadUnionSchema()
  if (mergeErrors.length > 0) {
    throw new Error(`schema merge errors:\n${mergeErrors.map((e) => `${e.path}: ${e.message}`).join('\n')}`)
  }
  const corpus = loadCorpus()
  const validation = validateCorpus(corpus, schema)
  if (validation.length > 0) {
    throw new Error(`document validation errors:\n${validation.map((e) => `${e.path}: ${e.message}`).join('\n')}`)
  }
  return generateOperationsSource(corpus.parsed, schema)
}

export const OPERATIONS_ARTIFACT = join(GRAPHQL_DIR, 'operations.ts')
