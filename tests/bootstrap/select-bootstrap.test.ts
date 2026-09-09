import { describe, expect, it, vi } from 'vitest'
import { runSelectedBootstrap } from '@/bootstrap/select-bootstrap'

describe('runSelectedBootstrap', () => {
  it('loads only the capture bootstrap for the native capture path', async () => {
    const loadFullApp = vi.fn(async () => undefined)
    const loadScreenCapture = vi.fn(async () => undefined)

    await runSelectedBootstrap('/screen-capture', true, {
      loadFullApp,
      loadScreenCapture,
    })

    expect(loadScreenCapture).toHaveBeenCalledOnce()
    expect(loadFullApp).not.toHaveBeenCalled()
  })

  it.each([
    ['/', true],
    ['/messages', true],
    ['/screen-capture', false],
  ])('loads only the full app for path %s in native=%s', async (pathname, isTauri) => {
    const loadFullApp = vi.fn(async () => undefined)
    const loadScreenCapture = vi.fn(async () => undefined)

    await runSelectedBootstrap(pathname, isTauri, {
      loadFullApp,
      loadScreenCapture,
    })

    expect(loadFullApp).toHaveBeenCalledOnce()
    expect(loadScreenCapture).not.toHaveBeenCalled()
  })

  it('propagates bootstrap failure instead of attempting the other application', async () => {
    const expected = new Error('capture bootstrap failed')
    const loadFullApp = vi.fn(async () => undefined)
    const loadScreenCapture = vi.fn(async () => {
      throw expected
    })

    await expect(
      runSelectedBootstrap('/screen-capture', true, {
        loadFullApp,
        loadScreenCapture,
      })
    ).rejects.toBe(expected)

    expect(loadFullApp).not.toHaveBeenCalled()
  })
})
