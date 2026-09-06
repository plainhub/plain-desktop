import { describe, expect, it } from 'vitest'
import { isTauriBuildMode } from '../../build-support/app-mode'

describe('isTauriBuildMode', () => {
  it('selects Tauri for the cross-platform Vite mode used by packaged builds', () => {
    expect(isTauriBuildMode('tauri', undefined)).toBe(true)
  })

  it('keeps the legacy environment override used by CI and existing tooling', () => {
    expect(isTauriBuildMode('production', 'tauri')).toBe(true)
  })

  it('keeps an ordinary production build in browser mode', () => {
    expect(isTauriBuildMode('production', undefined)).toBe(false)
  })
})
