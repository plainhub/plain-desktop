import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import { expect, it, vi } from 'vitest'
import { fileIconAssets } from '../../build-support/file-icons'

it('emits the Core icon snapshot even after Vite empties the output directory', () => {
  const root = mkdtempSync(join(tmpdir(), 'plain-icons-'))
  const directory = join(root, 'ficons')
  mkdirSync(directory)
  for (const name of ['folder', 'default', 'pdf']) writeFileSync(join(directory, `${name}.svg`), `<svg>${name}</svg>`)
  const plugin = fileIconAssets(pathToFileURL(`${directory}/`))
  rmSync(root, { recursive: true })
  const emitFile = vi.fn()
  ;(plugin.generateBundle as Function).call({ emitFile })
  expect(emitFile.mock.calls.map(([asset]) => asset.fileName)).toEqual(['ficons/default.svg', 'ficons/folder.svg', 'ficons/pdf.svg'])
  expect(emitFile.mock.calls[2][0].source.toString()).toBe('<svg>pdf</svg>')
})

it('rejects an incomplete Core icon set before Vite can clear the output', () => {
  const root = mkdtempSync(join(tmpdir(), 'plain-icons-'))
  try {
    writeFileSync(join(root, 'default.svg'), '<svg/>')
    expect(() => fileIconAssets(pathToFileURL(`${root}/`))).toThrow('incomplete')
  } finally { rmSync(root, { recursive: true }) }
})

it('keeps canonical icons independent of the generated Web directory', () => {
  const root = mkdtempSync(join(tmpdir(), 'plain-icons-source-'))
  try {
    const source = join(root, 'assets', 'ficons')
    const output = join(root, 'web_dist')
    mkdirSync(source, { recursive: true })
    mkdirSync(output)
    for (const name of ['default', 'folder']) writeFileSync(join(source, `${name}.svg`), `<svg>${name}</svg>`)
    const plugin = fileIconAssets(pathToFileURL(`${source}/`))
    rmSync(output, { recursive: true })
    const emitFile = vi.fn()
    ;(plugin.generateBundle as Function).call({ emitFile })
    expect(readFileSync(join(source, 'default.svg'), 'utf8')).toBe('<svg>default</svg>')
    expect(emitFile.mock.calls).toHaveLength(2)
  } finally { rmSync(root, { recursive: true }) }
})
