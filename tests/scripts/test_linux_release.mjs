import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

const root = new URL('../../', import.meta.url)

test('Linux release artifacts are built and smoke-tested on the Ubuntu 24.04 baseline', () => {
  const workflow = readFileSync(new URL('.github/workflows/build-desktop-release.yml', root), 'utf8')
  assert.match(workflow, /- platform: linux\n\s+os: ubuntu-24\.04\b/)
  assert.match(workflow, /for root in "\$temp\/deb" "\$temp\/rpm" "\$temp\/squashfs-root"; do[\s\S]*?bash scripts\/check-linux-glibc\.sh "\$binary"/)
  assert.match(workflow, /linux-smoke:\n[\s\S]*?runs-on: ubuntu-24\.04\b/)
  assert.match(workflow, /provenance:\n[\s\S]*?needs: \[build, linux-smoke\]/)
})

function check(report) {
  return spawnSync('bash', ['-c', `
    readelf() { printf '%s\\n' "$ELF_REPORT"; }
    export -f readelf
    bash scripts/check-linux-glibc.sh plainapp-desktop
  `], { cwd: root, env: { ...process.env, ELF_REPORT: report }, encoding: 'utf8' })
}

test('the glibc gate accepts the supported ABI and rejects the Preview dependency', () => {
  const definitions = 'Version definition section:\n  Name: GLIBC_2.43\n'
  const imports = 'Version needs section:\n  File: libm.so.6  Cnt: 2\n  Name: GLIBC_2.2.5\n'
  const supported = check(definitions + imports + '  Name: GLIBC_2.39')
  assert.equal(supported.status, 0, supported.stderr)
  const tooNew = check(imports + '  Name: GLIBC_2.43')
  assert.equal(tooNew.status, 1)
  assert.match(tooNew.stderr, /GLIBC_2\.43/)
  assert.equal(check(imports + '  Name: GLIBC_2.39.1').status, 1)
  assert.equal(check(definitions).status, 1)
})
