# API tests

`helpers.ts` sends encrypted GraphQL requests. `api.test.ts` runs the same cases against every configured target. Each mutation cleans up its own IDs with `finally`.

1. Copy `targets.example.json` to `targets.local.json`; fill in each API base URL and authenticated session `clientId`.
2. Copy `.env.example` to `.env.local`; fill in the base64 32-byte session keys named by `keyEnv`.
3. Run `yarn test:api` from the repository root.

Use dedicated test instances. Configure optional `caFile` for a server certificate, or `insecure: true` for an explicitly insecure test target. Certificate paths are relative to `api-test/`; TLS options apply only to that target.

`API_TEST_TARGETS=desktop,nas yarn test:api` selects targets. `API_TEST_CONFIG` selects another target JSON file. Missing credentials, unavailable servers and contract failures fail the run. Target cases run concurrently, with at most three tests active.

For a JSON report, run `yarn test:api --reporter=json --outputFile=reports/results.json`. Check types with `yarn test:api:typecheck`.

The previous framework is preserved under `trash/api-tests/framework/` for review. Core Rust unit tests stay in their crates.
