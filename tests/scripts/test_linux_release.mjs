import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

const root = new URL('../../', import.meta.url)

test('Linux release artifacts are built and smoke-tested on the Ubuntu 24.04 baseline', () => {
  const workflow = readFileSync(new URL('.github/workflows/build-desktop-release.yml', root), 'utf8')
  assert.match(workflow, /- platform: linux\n\s+os: ubuntu-24\.04\b/)
  assert.match(workflow, /linux-smoke:\n[\s\S]*?runs-on: ubuntu-24\.04\b/)
  assert.match(workflow, /provenance:\n[\s\S]*?needs: \[build, linux-smoke\]/)
})

test('the Linux installer check streams the RPM executable and reports each package', () => {
  const workflow = readFileSync(new URL('.github/workflows/build-desktop-release.yml', root), 'utf8')
  const step = workflow.split('      - name: Verify Linux installers\n')[1].split('      - name:')[0]
  const script = step.split('        run: |\n')[1].trimEnd()
  const checked = spawnSync('bash', ['-c', `
    dpkg-deb() {
      mkdir -p "$3/usr/bin"
      cp /bin/sh "$3/usr/bin/plainapp-desktop"
    }
    rpm2cpio() { printf 'archive'; }
    cpio() {
      [[ "$*" == *--to-stdout* && "$*" == *./usr/bin/plainapp-desktop* ]] || return 1
      cat >/dev/null
      cat /bin/sh
    }
    appimage() {
      mkdir -p squashfs-root/usr/bin
      cp /bin/sh squashfs-root/usr/bin/plainapp-desktop
    }
    bash() {
      shift
      for binary; do test -s "$binary" && printf 'checked %s\\n' "$binary" || return 1; done
    }
    realpath() {
      case "$1" in
        *.AppImage) printf 'appimage\\n' ;;
        *.rpm) printf 'test.rpm\\n' ;;
      esac
    }
    ${script}
  `], { cwd: root, encoding: 'utf8' })
  assert.equal(checked.status, 0, checked.stdout + checked.stderr)
  assert.match(checked.stdout, /checked .*deb/)
  assert.match(checked.stdout, /checked .*rpm/)
  assert.match(checked.stdout, /checked .*squashfs-root/)
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
  assert.match(supported.stdout, /requires at most GLIBC_2\.39/)
  const tooNew = check(imports + '  Name: GLIBC_2.43')
  assert.equal(tooNew.status, 1)
  assert.match(tooNew.stderr, /GLIBC_2\.43/)
  assert.equal(check(imports + '  Name: GLIBC_2.39.1').status, 1)
  assert.equal(check(definitions).status, 1)
})
