import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

const root = new URL('../../', import.meta.url)
const read = path => readFileSync(new URL(path, root), 'utf8')

test('the dev profile skips the optimizer for business packages', () => {
  const profiles = new Map(read('Cargo.toml').split(/^\[([^\]\r\n]+)\]\s*$/m).slice(1)
    .flatMap((part, index, parts) => index % 2 ? [] : [[part, parts[index + 1]]]))
  assert.match(profiles.get('profile.dev'), /^opt-level\s*=\s*0$/m)
  assert.match(profiles.get('profile.dev'), /^incremental\s*=\s*true$/m)
  for (const name of ['plain-rs', 'plain-server']) {
    assert.match(profiles.get(`profile.dev.package.${name}`), /^opt-level\s*=\s*0$/m)
  }
  assert.match(profiles.get('profile.dev.package.plain-libs'), /^opt-level\s*=\s*1$/m)
  assert.match(profiles.get('profile.dev.package."*"'), /^opt-level\s*=\s*3$/m)
})

test('the shell emits only the Rust library consumed by the executable', () => {
  const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps', '--offline', '--locked'], {
    cwd: root,
    encoding: 'utf8',
  }))
  const app = metadata.packages.find(pkg => pkg.name === 'PlainApp')
  assert.deepEqual(app.targets.find(target => target.name === 'plainapp_lib').crate_types, ['rlib'])
})

test('the preview packages without release optimization', () => {
  const { scripts } = JSON.parse(read('package.json'))
  assert.equal(scripts['build:tauri:preview'], 'tauri build --debug')
  assert.equal(scripts['build:tauri'], 'tauri build')
})
