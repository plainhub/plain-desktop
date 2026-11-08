import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { delimiter, join } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'

const linker = fileURLToPath(new URL('../../src-tauri/scripts/mac-dev-linker.sh', import.meta.url))
const scripts = {
  cc: 'printf "%s\\n" "$@" > "$LINKER_ARGS"\nprintf "%s" "$LINKER_MESSAGE" >&2\nexit "$LINKER_STATUS"',
  security: 'printf "%s" "$SIGNING_IDENTITY"',
  codesign: `printf "%s\\n" "$@" > "$SIGNING_ARGS"
if [ -n "$SIGNING_MESSAGE" ]; then
  printf "%s" "$SIGNING_MESSAGE" >&2
else
  printf "%s: replacing existing signature\\n" "\${!#}" >&2
fi
exit "$SIGNING_STATUS"`,
}

function runLinker(t, { identity = true, linkerStatus = 0, signingStatus = 0, app = true, signingWarning = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'plain-mac-linker-'))
  t.after(() => rmSync(root, { recursive: true }))
  for (const [name, script] of Object.entries(scripts)) {
    writeFileSync(join(root, name), `#!/bin/bash\n${script}\n`, { mode: 0o755 })
  }
  const linkerArgs = join(root, 'linker-args')
  const signingArgs = join(root, 'signing-args')
  const output = join(root, app ? 'target/debug/deps/plainapp_desktop-test' : 'other')
  const result = spawnSync('/bin/bash', [linker, '-o', output], {
    encoding: 'utf8',
    env: {
      ...process.env,
      PATH: root + delimiter + process.env.PATH,
      LINKER_ARGS: linkerArgs,
      SIGNING_ARGS: signingArgs,
      LINKER_STATUS: String(linkerStatus),
      SIGNING_STATUS: String(signingStatus),
      SIGNING_IDENTITY: identity ? '1) "Apple Development: Test"\n' : '',
      LINKER_MESSAGE: linkerStatus ? 'linker failed\n' : '',
      SIGNING_MESSAGE: signingStatus ? 'signing failed\n' : signingWarning ? 'signing warning\n' : '',
    },
  })
  assert.ifError(result.error)
  return {
    ...result,
    args: readFileSync(linkerArgs, 'utf8').trimEnd().split('\n'),
    signed: existsSync(signingArgs) ? readFileSync(signingArgs, 'utf8').trimEnd().split('\n') : null,
  }
}

test('successful signing is silent and retains the stable identity', t => {
  const { status, stdout, stderr, args, signed } = runLinker(t)
  assert.deepEqual([status, stdout, stderr], [0, '', ''])
  assert.ok(args.includes('-Wl,-headerpad,0x8000'))
  assert.ok(signed.includes('Apple Development: Test'))
  assert.ok(signed.includes('com.ismartcoding.plain.desktop.dev'))
})

test('a signing failure preserves its diagnostic and exit status', t => {
  const { status, stderr } = runLinker(t, { signingStatus: 7 })
  assert.equal(status, 7)
  assert.match(stderr, /signing failed/)
})

test('a linker failure is preserved without signing', t => {
  const { status, stderr, signed } = runLinker(t, { linkerStatus: 12 })
  assert.equal(status, 12)
  assert.match(stderr, /linker failed/)
  assert.equal(signed, null)
})

test('unexpected successful signing diagnostics are preserved', t => {
  const { status, stderr } = runLinker(t, { signingWarning: true })
  assert.equal(status, 0)
  assert.match(stderr, /signing warning/)
})

for (const [name, options] of [['no identity', { identity: false }], ['other binaries', { app: false }]]) {
  test(`${name} leaves the default signature alone`, t => {
    const { status, stdout, stderr, args, signed } = runLinker(t, options)
    assert.deepEqual([status, stdout, stderr], [0, '', ''])
    assert.equal(args.includes('-Wl,-headerpad,0x8000'), options.app !== false)
    assert.equal(signed, null)
  })
}
