import { ESLint } from 'eslint'
import { expect, it } from 'vitest'

const eslint = new ESLint()

it.each([
  ['src/lint-fixture.js', 'export const double = (value) => value * 2'],
  ['src/lint-fixture.ts', 'export const double = (value: number): number => value * 2'],
  ['src/lint-fixture.tsx', 'export const Button = () => <button type="button">Save</button>'],
  [
    'src/LintFixture.vue',
    '<script setup lang="ts">const value: number = 1</script><template><div>{{ value }}</div></template>',
  ],
])('accepts valid source in %s', async (filePath, source) => {
  const [result] = await eslint.lintText(source, { filePath })
  expect(result.errorCount).toBe(0)
  expect(result.fatalErrorCount).toBe(0)
})

it('reports JavaScript correctness errors', async () => {
  const [result] = await eslint.lintText('export const invalid = (value) => value === NaN', {
    filePath: 'src/lint-fixture.js',
  })
  expect(result.messages).toEqual(expect.arrayContaining([expect.objectContaining({ ruleId: 'use-isnan', severity: 2 })]))
})

it('rejects invalid TypeScript syntax', async () => {
  const [result] = await eslint.lintText('export const value: = 1', { filePath: 'src/lint-fixture.ts' })
  expect(result.fatalErrorCount).toBe(1)
})

it('checks Vue template correctness', async () => {
  const [result] = await eslint.lintText('<template><div v-for="item in [1, 2]">{{ item }}</div></template>', {
    filePath: 'src/LintFixture.vue',
  })
  expect(result.messages).toEqual(
    expect.arrayContaining([expect.objectContaining({ ruleId: 'vue/require-v-for-key', severity: 2 })])
  )
})

it('applies the current recommended checks for unassigned variables', async () => {
  const [result] = await eslint.lintText('let value; export const read = () => value', { filePath: 'src/lint-fixture.js' })
  expect(result.messages).toEqual(
    expect.arrayContaining([expect.objectContaining({ ruleId: 'no-unassigned-vars', severity: 2 })])
  )
})
