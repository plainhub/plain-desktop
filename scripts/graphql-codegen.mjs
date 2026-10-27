import { readFile, writeFile, readdir } from 'node:fs/promises'
import { readApiSchema } from '../../plain-core/scripts/read-api-schema.mjs'
import { fileURLToPath } from 'node:url'
import { extractDocuments } from '../src/lib/api/graphql/extract.ts'
import { parseDocument, parseSchema } from '../src/lib/api/graphql/parser.ts'
import { validateDocument } from '../src/lib/api/graphql/schema.ts'
import { subscriptionOperations } from '../src/lib/api/subscription-operations.ts'
import { homeStatsGQL, HOME_STATS_COUNT_FIELDS } from '../src/lib/api/home-stats.ts'
import { generateOperationsSource } from '../src/lib/api/graphql/codegen.ts'

const root = new URL('../', import.meta.url)
const files = new Map()
async function scan(directory = '') {
  for (const entry of await readdir(new URL(`src/${directory}`, root), { withFileTypes: true })) {
    const path = `${directory}${entry.name}`
    if (entry.isDirectory()) await scan(`${path}/`)
    else if (/\.(ts|vue)$/.test(path)) files.set(path, await readFile(new URL(`src/${path}`, root), 'utf8'))
  }
}
await scan()
const extracted = extractDocuments(files)
const errors = [...extracted.errors]
for (const warning of extracted.warnings.filter((item) => item.file !== 'lib/api/home-stats.ts' || !['fields', 'variables'].includes(item.expr))) errors.push(`${warning.file}:${warning.line}: unresolved ${warning.expr}`)
const schema = parseSchema(readApiSchema())
const documents = []
const keys = new Map()
const mainDocuments = extracted.docs.filter((doc) => doc.file !== 'lib/api/guest.ts')
mainDocuments.push(...subscriptionOperations.map(operation => ({ key: `${operation.field}Subscription`, file: 'lib/api/subscription-operations.ts', text: operation.document })))
mainDocuments.push({ key: 'homeStatsGQL', file: 'lib/api/home-stats.ts', text: homeStatsGQL(Object.keys(HOME_STATS_COUNT_FIELDS)) })
for (const doc of mainDocuments) {
  const ir = parseDocument(doc.text)
  for (const error of validateDocument(ir, schema, doc.key)) errors.push(`${doc.file}: ${error.path}: ${error.message}`)
  if (ir.operations.length === 0) continue
  if (keys.has(doc.key)) {
    if (keys.get(doc.key) !== doc.text) errors.push(`${doc.file}: conflicting operation key ${doc.key}`)
    continue
  }
  keys.set(doc.key, doc.text)
  documents.push({ key: doc.key, ir })
}
if (errors.length) {
  console.error(errors.join('\n'))
  process.exitCode = 1
} else {
  const output = new URL('src/lib/api/graphql/operations.ts', root)
  const source = generateOperationsSource(documents.sort((a, b) => a.key.localeCompare(b.key)), schema)
  if (process.argv.includes('--check')) {
    if (await readFile(output, 'utf8') !== source) {
      console.error(`${fileURLToPath(output)} needs regeneration`)
      process.exitCode = 1
    }
  } else await writeFile(output, source)
  console.log(`Validated ${mainDocuments.length} main-schema documents and ${documents.length} operations`)
}
