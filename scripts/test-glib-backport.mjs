import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../', import.meta.url))
const source = resolve(process.env.GLIB_BACKPORT_SOURCE ?? join(root, 'vendor/glib'))
const originalLock = join(root, 'Cargo.lock')
const directory = mkdtempSync(join(tmpdir(), 'plain-glib-regression-'))
const lock = join(directory, 'Cargo.lock')
const readPackages = path => readFileSync(path, 'utf8').split(/^\[\[package\]\]\s*$/m).slice(1).map(entry => {
  const name = entry.match(/^name = "([^"]+)"$/m)?.[1]
  const identity = entry.match(/^(?:name|version|source|checksum) = ".*"$/gm).join('\n')
  return { name, identity }
})

try {
  writeFileSync(join(directory, 'Cargo.toml'), `[package]
name = "plain-glib-regression"
version = "0.0.0"
edition = "2021"
[dependencies]
glib = { path = ${JSON.stringify(source)} }
[[test]]
name = "variant"
path = ${JSON.stringify(join(root, 'tests/security/glib_variant.rs'))}
`)
  copyFileSync(originalLock, lock)
  execFileSync('cargo', ['metadata', '--offline', '--format-version', '1'], { cwd: directory, stdio: ['ignore', 'ignore', 'inherit'] })
  const allowed = new Set(readPackages(originalLock).map(pkg => pkg.identity))
  const added = readPackages(lock).filter(pkg => pkg.name !== 'plain-glib-regression' && !allowed.has(pkg.identity))
  if (added.length) throw new Error(`Unexpected dependencies:\n${added.map(pkg => pkg.identity).join('\n\n')}`)
  execFileSync('cargo', ['test', '--offline', '--locked', '--release', '--test', 'variant'], { cwd: directory, stdio: 'inherit' })
} finally {
  rmSync(directory, { recursive: true })
}
