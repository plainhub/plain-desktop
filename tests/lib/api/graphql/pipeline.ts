import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { extractDocuments, type ExtractedDoc, type ExtractResult } from '../../../../src/lib/api/graphql/extract'
import { parseDocument, parseSchema, type DocumentIR, type SchemaIR, type TypeDef } from '../../../../src/lib/api/graphql/parser'
import { validateDocument, type ValidationError } from '../../../../src/lib/api/graphql/schema'
import { generateOperationsSource } from '../../../../src/lib/api/graphql/codegen'
import { homeStatsGQL } from '../../../../src/lib/api/query'

export const ROOT = process.cwd()
const SRC_DIR = join(ROOT, 'src')
const GRAPHQL_DIR = join(SRC_DIR, 'lib/api/graphql')
const EXTERNAL_API_DOCS = new Set(['sharedInfoGQL'])

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

export function loadUnionSchema(): SchemaIR {
  const rustSchema = parseSchema(readFileSync(join(ROOT, 'schema/schema.graphql'), 'utf8'))
  const appSchema = parseSchema(readFileSync(join(ROOT, 'tests/lib/api/graphql/contracts/plain-app.graphqls'), 'utf8'))
  const schema = cloneSchema(rustSchema)
  for (const [name, appType] of appSchema.types) {
    const rustName = name === appSchema.queryRoot ? rustSchema.queryRoot : name === appSchema.mutationRoot ? rustSchema.mutationRoot : name
    const rustType = schema.types.get(rustName)
    if (!rustType) {
      schema.types.set(rustName, cloneType({ ...appType, name: rustName }))
      continue
    }
    for (const field of appType.fields) {
      if (!rustType.fields.some((candidate) => candidate.name === field.name)) {
        rustType.fields.push(field)
      }
    }
    for (const value of appType.enumValues) {
      if (!rustType.enumValues.includes(value)) rustType.enumValues.push(value)
    }
  }
  return schema
}

function cloneType(type: TypeDef): TypeDef {
  return { ...type, fields: [...type.fields], interfaces: [...type.interfaces], unionMembers: [...type.unionMembers], enumValues: [...type.enumValues] }
}

function cloneSchema(schema: SchemaIR): SchemaIR {
  return {
    types: new Map([...schema.types].map(([name, type]) => [name, cloneType(type)])),
    queryRoot: schema.queryRoot,
    mutationRoot: schema.mutationRoot,
    subscriptionRoot: schema.subscriptionRoot,
  }
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
    if (EXTERNAL_API_DOCS.has(doc.key)) continue
    errors.push(...validateDocument(doc.ir, schema, doc.key))
  }
  return errors
}

export function generateOperationsArtifact(): string {
  const schema = loadUnionSchema()
  const corpus = loadCorpus()
  const validation = validateCorpus(corpus, schema)
  if (validation.length > 0) {
    throw new Error(`document validation errors:\n${validation.map((e) => `${e.path}: ${e.message}`).join('\n')}`)
  }
  return generateOperationsSource(corpus.parsed.filter((doc) => !EXTERNAL_API_DOCS.has(doc.key)), schema)
}

export const OPERATIONS_ARTIFACT = join(GRAPHQL_DIR, 'operations.ts')
