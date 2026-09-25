// Generates `docs/DATABASE.sql` from the machine-readable source of truth:
// the CREATE TABLE / CREATE INDEX statements in
// `src-tauri/src/local/db/mod.rs` (the local-mode SQLite schema).
//
// The file is generated, never hand-edited. The freshness test
// (`tests/docs/db-schema.test.ts`, vitest `docs` project) fails when it goes
// stale; regenerate with `node scripts/gen-db-schema-sql.mjs --write` and
// commit the file together with the schema change.

import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const ROOT = process.cwd()

export function generateDatabaseSql() {
  const src = readFileSync(join(ROOT, 'src-tauri/src/local/db/mod.rs'), 'utf8')
  const stmts = [...src.matchAll(/(CREATE (?:TABLE|UNIQUE INDEX|INDEX) IF NOT EXISTS [\s\S]*?);/g)].map((m) =>
    // Dedent: keep the author's multi-line layout, drop the Rust-string indent.
    m[1]
      .split('\n')
      .map((l) => l.trimEnd())
      .filter((l, i) => l.trim() !== '' || i > 0)
      .map((l) => l.trimStart())
      .join('\n')
      .trim() + ';',
  )
  const header = [
    '-- plain-desktop 本地 SQLite DDL（local_chat.db，WAL）',
    '-- 自动生成，禁止手改。源：src-tauri/src/local/db/mod.rs',
    '-- 再生：node scripts/gen-db-schema-sql.mjs --write',
    '-- 过期锁：yarn docs:check（vitest docs project）',
    '',
  ]
  return header.join('\n') + stmts.join('\n') + '\n'
}

const FILE = 'docs/DATABASE.sql'

if (process.argv[1] && process.argv[1].endsWith('gen-db-schema-sql.mjs')) {
  if (process.argv.includes('--write')) {
    writeFileSync(join(ROOT, FILE), generateDatabaseSql())
    console.log(`wrote ${FILE}`)
  } else {
    const committed = readFileSync(join(ROOT, FILE), 'utf8')
    if (committed !== generateDatabaseSql()) {
      console.error(`${FILE} is stale. Run \`node scripts/gen-db-schema-sql.mjs --write\` and commit.`)
      process.exit(1)
    }
    console.log(`${FILE} is up to date`)
  }
}
