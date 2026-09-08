import { afterEach, describe, expect, it, vi } from 'vitest'
import { copyTextToClipboard } from '@/lib/clipboard'

const nav = navigator as Navigator & { clipboard?: Clipboard }
const originalClipboard = Object.getOwnPropertyDescriptor(nav, 'clipboard')
const originalExecCommand = Object.getOwnPropertyDescriptor(document, 'execCommand')

function stubClipboard(clipboard: Partial<Clipboard> | undefined) {
  Object.defineProperty(nav, 'clipboard', { value: clipboard, configurable: true })
}

function stubExecCommand(impl: () => boolean) {
  const fn = vi.fn(impl)
  Object.defineProperty(document, 'execCommand', { value: fn, configurable: true })
  return fn
}

afterEach(() => {
  if (originalClipboard) Object.defineProperty(nav, 'clipboard', originalClipboard)
  else stubClipboard(undefined)
  if (originalExecCommand) Object.defineProperty(document, 'execCommand', originalExecCommand)
  else delete (document as any).execCommand
  document.querySelectorAll('textarea').forEach((ta) => ta.remove())
})

describe('copyTextToClipboard', () => {
  it('uses the async clipboard API when available', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    stubClipboard({ writeText } as Partial<Clipboard>)
    const exec = stubExecCommand(() => true)

    await expect(copyTextToClipboard('hello')).resolves.toBe(true)
    expect(writeText).toHaveBeenCalledWith('hello')
    expect(exec).not.toHaveBeenCalled()
  })

  it('falls back to execCommand when writeText rejects', async () => {
    const writeText = vi.fn().mockRejectedValue(new Error('denied'))
    stubClipboard({ writeText } as Partial<Clipboard>)
    const exec = stubExecCommand(() => true)

    await expect(copyTextToClipboard('hello')).resolves.toBe(true)
    expect(exec).toHaveBeenCalled()
  })

  it('falls back to execCommand when the clipboard API is unavailable', async () => {
    stubClipboard(undefined)
    const exec = stubExecCommand(() => true)

    await expect(copyTextToClipboard('hello')).resolves.toBe(true)
    expect(exec).toHaveBeenCalled()
  })

  it('returns false when execCommand fails', async () => {
    stubClipboard(undefined)
    stubExecCommand(() => false)

    await expect(copyTextToClipboard('hello')).resolves.toBe(false)
  })

  it('returns false when execCommand throws', async () => {
    stubClipboard(undefined)
    stubExecCommand(() => {
      throw new Error('not supported')
    })

    await expect(copyTextToClipboard('hello')).resolves.toBe(false)
  })

  it('removes the helper textarea afterwards', async () => {
    stubClipboard(undefined)
    stubExecCommand(() => true)

    await copyTextToClipboard('hello')
    expect(document.querySelectorAll('textarea')).toHaveLength(0)
  })
})
