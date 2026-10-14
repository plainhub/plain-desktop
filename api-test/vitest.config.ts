import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  resolve: { alias: { '@': fileURLToPath(new URL('../src', import.meta.url)) } },
  test: { environment: 'node', include: ['*.test.ts', 'tests/unit/**/*.test.ts'], testTimeout: 60000, maxConcurrency: 3 },
})
