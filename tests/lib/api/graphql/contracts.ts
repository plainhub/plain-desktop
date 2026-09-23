import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  parseSchema,
  type SchemaIR,
  type TypeDef,
  type TypeRef,
} from '../../../../src/lib/api/graphql/parser'
import { mergeSchemas, validateDocument, type ValidationError } from '../../../../src/lib/api/graphql/schema'
import { loadCorpus, type Corpus } from './pipeline'

export const CONTRACTS_DIR = join(process.cwd(), 'tests/lib/api/graphql/contracts')

export type BackendName = 'plain-app' | 'plain-nas'

export function loadServerSchema(backend: BackendName): SchemaIR {
  return parseSchema(readFileSync(join(CONTRACTS_DIR, `${backend}.graphqls`), 'utf8'))
}

export function loadMergedSchema(): SchemaIR {
  const local = parseSchema(readFileSync(join(process.cwd(), 'src/lib/api/graphql/local-schema.graphql'), 'utf8'))
  const extension = parseSchema(readFileSync(join(process.cwd(), 'src/lib/api/graphql/extension-schema.graphql'), 'utf8'))
  const merged = mergeSchemas(local, extension)
  if (merged.errors.length > 0) {
    throw new Error(`schema merge errors:\n${merged.errors.map((e) => `${e.path}: ${e.message}`).join('\n')}`)
  }
  return merged.schema
}

export interface CorpusIssue {
  doc: string
  error: ValidationError
}

export function validateCorpusAgainst(corpus: Corpus, schema: SchemaIR): CorpusIssue[] {
  const out: CorpusIssue[] = []
  for (const doc of corpus.parsed) {
    for (const error of validateDocument(doc.ir, schema, doc.key)) {
      out.push({ doc: doc.key, error })
    }
  }
  return out
}

export function rootFieldOf(issue: CorpusIssue): string | undefined {
  const afterColon = issue.error.path.split(':').pop() ?? ''
  const segments = afterColon.split('.')
  const rootField = segments.length >= 2 ? segments[1] : undefined
  if (rootField) return rootField.split(' ')[0].replace(/·.*$/, '')
  return undefined
}

function wireKind(name: string): string {
  if (name === 'String' || name === 'ID' || name === 'Instant') return 'string'
  if (name === 'Int' || name === 'Long' || name === 'Float') return 'number'
  return name
}

interface ShapeIssue {
  path: string
  message: string
}

function normalizeRef(ref: TypeRef): string {
  if (ref.ofType === 'named') {
    const kind = wireKind(ref.name)
    return ref.nonNull ? `${kind}!` : kind
  }
  const inner = normalizeRef(ref.inner)
  return ref.ofType === 'list' ? (ref.nonNull ? `[${inner}]!` : `[${inner}]`) : inner
}

function compareFieldShape(
  path: string,
  desk: { type: TypeRef; args: { name: string; type: TypeRef; hasDefault: boolean }[] },
  srv: { type: TypeRef; args: { name: string; type: TypeRef; hasDefault: boolean }[] },
  deskSchema: SchemaIR,
  srvSchema: SchemaIR,
  enumExtraAllowlist: Record<string, string[]>,
  out: ShapeIssue[],
  fieldWaiver: Set<string>,
): void {
  if (fieldWaiver.has(path)) return
  for (const a of desk.args) {
    const b = srv.args.find((x) => x.name === a.name)
    if (!b) {
      out.push({ path, message: `argument '${a.name}' missing on the server` })
      continue
    }
    if (normalizeRef(a.type) !== normalizeRef(b.type)) {
      out.push({ path, message: `argument '${a.name}': desktop ${refStr(a.type)} vs server ${refStr(b.type)}` })
    }
  }
  if (normalizeRef(desk.type) !== normalizeRef(srv.type)) {
    const deskNamed = namedOf(desk.type)
    const srvNamed = namedOf(srv.type)
    const deskDef = deskSchema.types.get(deskNamed)
    const srvDef = srvSchema.types.get(srvNamed)
    if (deskDef && srvDef && enumOrUnionCompatible(deskDef, srvDef, enumExtraAllowlist, deskNamed)) return
    out.push({ path, message: `returns desktop ${refStr(desk.type)} but server has ${refStr(srv.type)}` })
  }
}

function refStr(ref: TypeRef): string {
  if (ref.ofType === 'named') return ref.nonNull ? `${ref.name}!` : ref.name
  const inner = refStr(ref.inner)
  return ref.ofType === 'list' ? (ref.nonNull ? `[${inner}]!` : `[${inner}]`) : inner
}

function namedOf(ref: TypeRef): string {
  return ref.ofType === 'named' ? ref.name : namedOf(ref.inner)
}

function enumOrUnionCompatible(
  a: TypeDef,
  b: TypeDef,
  enumExtraAllowlist: Record<string, string[]>,
  deskName: string,
): boolean {
  if (a.kind === 'enum' && b.kind === 'enum') {
    const extras = a.enumValues.filter((v) => !b.enumValues.includes(v))
    const allowed = enumExtraAllowlist[deskName] ?? []
    return extras.every((v) => allowed.includes(v)) && b.enumValues.every((v) => a.enumValues.includes(v))
  }
  if (a.kind === 'union' && b.kind === 'union') {
    return a.unionMembers.every((m) => b.unionMembers.includes(m)) && b.unionMembers.every((m) => a.unionMembers.includes(m))
  }
  return false
}

export interface ShapeComparison {
  issues: ShapeIssue[]
  compared: number
}

export function compareShapes(
  deskSchema: SchemaIR,
  srvSchema: SchemaIR,
  srvName: string,
  enumExtraAllowlist: Record<string, string[]>,
  fieldWaiver: Set<string> = new Set(),
): ShapeComparison {
  const issues: ShapeIssue[] = []
  let compared = 0
  const compareType = (name: string, deskDef: TypeDef | undefined, srvDef: TypeDef | undefined, isRoot = false) => {
    if (!deskDef || !srvDef) return
    if (deskDef.kind !== srvDef.kind) {
      issues.push({ path: `[${srvName}] ${name}`, message: `${deskDef.kind} vs server ${srvDef.kind}` })
      return
    }
    compared++
    if (deskDef.kind === 'enum' && srvDef.kind === 'enum') {
      if (!enumOrUnionCompatible(deskDef, srvDef, enumExtraAllowlist, name)) {
        issues.push({
          path: `[${srvName}] ${name}`,
          message: `enum values desktop [${deskDef.enumValues}] vs server [${srvDef.enumValues}]`,
        })
      }
      return
    }
    if (deskDef.kind === 'union' && srvDef.kind === 'union') {
      if (!enumOrUnionCompatible(deskDef, srvDef, enumExtraAllowlist, name)) {
        issues.push({ path: `[${srvName}] ${name}`, message: `union members differ` })
      }
      return
    }
    for (const f of deskDef.fields) {
      const srvField = srvDef.fields.find((x) => x.name === f.name)
      if (!srvField) {
        // The desktop union contract spans local + phone + NAS surfaces; a
        // root field one backend doesn't serve is expected (feature gating),
        // a missing field on a shared type is real drift.
        if (!isRoot) {
          issues.push({ path: `[${srvName}] ${name}.${f.name}`, message: 'field missing on the server' })
        }
        continue
      }
      compareFieldShape(`[${srvName}] ${name}.${f.name}`, f, srvField, deskSchema, srvSchema, enumExtraAllowlist, issues, fieldWaiver)
    }
  }
  const rootNames = new Set([
    deskSchema.queryRoot,
    deskSchema.mutationRoot,
    srvSchema.queryRoot,
    srvSchema.mutationRoot,
  ])
  for (const [name, deskDef] of deskSchema.types) {
    if (rootNames.has(name)) continue
    compareType(name, deskDef, srvSchema.types.get(name))
  }
  compareType('Query', deskSchema.types.get(deskSchema.queryRoot), srvSchema.types.get(srvSchema.queryRoot), true)
  compareType('Mutation', deskSchema.types.get(deskSchema.mutationRoot), srvSchema.types.get(srvSchema.mutationRoot), true)
  return { issues, compared }
}
