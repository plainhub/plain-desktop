import { readFileSync, writeFileSync } from 'node:fs'

const source = new URL('../../plain-core/plain-rs/src/infrastructure/signature.rs', import.meta.url)
const target = new URL('../src/lib/api/signature-purpose.generated.ts', import.meta.url)
const rust = readFileSync(source, 'utf8')
const entries = [...rust.matchAll(/#\[serde\(rename = "([^"]+)"\)\]\s+(\w+),/g)]
if (!entries.length || new Set(entries.map(entry => entry[1])).size !== entries.length) throw new Error('Invalid signature purposes')
const output = '// Generated from plain-core SignaturePurpose. Run node scripts/signature-purpose-codegen.mjs --write.\n' +
  'export enum SignaturePurpose {\n' + entries.map(([, value, name]) => `  ${name} = '${value}',\n`).join('') + '}\n'
if (process.argv.includes('--write')) writeFileSync(target, output)
else if (readFileSync(target, 'utf8') !== output) throw new Error('Signature purposes are stale')
