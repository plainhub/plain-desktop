import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { generateDatabaseSql } from '../../scripts/gen-db-schema-sql.mjs'

// Locks docs/DATABASE.sql against its machine-readable source (the SQLite
// CREATE TABLE/INDEX statements in src-tauri/src/local/db/mod.rs): the
// committed file must equal the freshly generated output byte for byte.
// Regenerate with `node scripts/gen-db-schema-sql.mjs --write` and commit
// the file together with the schema change.

describe('generated db schema sql', () => {
  it('docs/DATABASE.sql is up to date', () => {
    const committed = readFileSync(join(process.cwd(), 'docs/DATABASE.sql'), 'utf8')
    expect(committed, 'docs/DATABASE.sql is stale. Run `node scripts/gen-db-schema-sql.mjs --write` and commit.').toBe(
      generateDatabaseSql(),
    )
  })
})
