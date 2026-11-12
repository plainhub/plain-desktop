import { readFileSync, writeFileSync } from 'node:fs'
import { aadPurposeModule } from '../../plain-core/scripts/aad-purpose-codegen.mjs'

const source = new URL('../../plain-core/plain-libs/src/aad.rs', import.meta.url)
const target = new URL('../src/lib/api/aad-purpose.generated.ts', import.meta.url)
const output = aadPurposeModule(readFileSync(source, 'utf8'))
if (process.argv.includes('--write')) writeFileSync(target, output)
else if (readFileSync(target, 'utf8') !== output) throw new Error('AAD purposes are stale')
