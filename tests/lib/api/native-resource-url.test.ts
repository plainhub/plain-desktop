import { describe, expect, it, vi, beforeEach } from 'vitest'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

let localAllowed = true
vi.mock('@/lib/device/local-mode', () => ({
  isLocalModeAllowed: () => localAllowed,
}))

import { closeNativeResource, isExternalReference, isNativeResourceUrl, MAX_NATIVE_RESPONSE_BYTES, nativeResourceUrl } from '@/hooks/native-resource-url'

function reset() {
  invoke.mockReset()
  localAllowed = true
}

describe('nativeResourceUrl', () => {
  beforeEach(reset)

  it('opens a handle and returns the protocol url', async () => {
    invoke.mockResolvedValue({ id: 'h1', url: 'plain-resource://file/h1', name: 'a.jpg', mime: 'image/jpeg', size: 10 })
    await expect(nativeResourceUrl('/data/a.jpg')).resolves.toBe('plain-resource://file/h1')
    expect(invoke).toHaveBeenCalledWith('native_resource_open', { path: '/data/a.jpg' })
  })

  it('reuses one handle per path', async () => {
    invoke.mockResolvedValue({ id: 'h1', url: 'plain-resource://file/h1', name: 'a.jpg', mime: 'image/jpeg', size: 10 })
    await nativeResourceUrl('/data/reuse.jpg')
    invoke.mockClear()
    await expect(nativeResourceUrl('/data/reuse.jpg')).resolves.toBe('plain-resource://file/h1')
    expect(invoke).not.toHaveBeenCalled()
  })

  it('de-duplicates concurrent opens of the same path', async () => {
    invoke.mockResolvedValue({ id: 'h1', url: 'plain-resource://file/h1', name: 'a.jpg', mime: 'image/jpeg', size: 10 })
    const [first, second] = await Promise.all([nativeResourceUrl('/data/twin.jpg'), nativeResourceUrl('/data/twin.jpg')])
    expect(first).toBe(second)
    expect(invoke).toHaveBeenCalledTimes(1)
  })

  it('returns empty off the local core', async () => {
    localAllowed = false
    await expect(nativeResourceUrl('/data/a.jpg')).resolves.toBe('')
    expect(invoke).not.toHaveBeenCalled()
  })

  it('returns empty for an empty path', async () => {
    await expect(nativeResourceUrl('')).resolves.toBe('')
    expect(invoke).not.toHaveBeenCalled()
  })

  it('hands shared-core references to the core', async () => {
    invoke.mockResolvedValue({ id: 'h1', url: 'plain-resource://file/h1', name: 'a.jpg', mime: 'image/jpeg', size: 10 })
    await expect(nativeResourceUrl('fid:abc123.jpg')).resolves.toBe('plain-resource://file/h1')
    await expect(nativeResourceUrl('app://photos/a.jpg')).resolves.toBe('plain-resource://file/h1')
    expect(invoke).toHaveBeenCalledWith('native_resource_open', { path: 'fid:abc123.jpg' })
    expect(invoke).toHaveBeenCalledWith('native_resource_open', { path: 'app://photos/a.jpg' })
  })

  it('fails when the core refuses a reference', async () => {
    invoke.mockRejectedValue('file is not authorized')
    await expect(nativeResourceUrl('fid:denied.jpg')).rejects.toBe('file is not authorized')
  })

  it('accepts absolute windows paths', async () => {
    invoke.mockResolvedValue({ id: 'h1', url: 'plain-resource://file/h1', name: 'a.jpg', mime: 'image/jpeg', size: 10 })
    await expect(nativeResourceUrl('C:\\data\\a.jpg')).resolves.toBe('plain-resource://file/h1')
    expect(invoke).toHaveBeenCalledTimes(1)
  })

  it('keeps files above the protocol response window on their existing transport', async () => {
    await expect(nativeResourceUrl('/data/big.mp4', MAX_NATIVE_RESPONSE_BYTES + 1)).resolves.toBe('')
    expect(invoke).not.toHaveBeenCalled()
  })

  it('opens files exactly at the response window', async () => {
    invoke.mockResolvedValue({ id: 'h1', url: 'plain-resource://file/h1', name: 'a.mp4', mime: 'video/mp4', size: 1 })
    await expect(nativeResourceUrl('/data/exact.mp4', MAX_NATIVE_RESPONSE_BYTES)).resolves.toBe('plain-resource://file/h1')
    expect(invoke).toHaveBeenCalledTimes(1)
  })
})

describe('closeNativeResource', () => {
  beforeEach(reset)

  it('closes the handle behind a cached url', async () => {
    invoke.mockResolvedValue({ id: 'h9', url: 'plain-resource://file/h9', name: 'a.jpg', mime: 'image/jpeg', size: 10 })
    await nativeResourceUrl('/data/close.jpg')
    invoke.mockClear()
    await closeNativeResource('/data/close.jpg')
    expect(invoke).toHaveBeenCalledWith('native_resource_close', { id: 'h9' })
  })

  it('reopens after the handle was closed', async () => {
    invoke.mockResolvedValue({ id: 'h9', url: 'plain-resource://file/h9', name: 'a.jpg', mime: 'image/jpeg', size: 10 })
    await nativeResourceUrl('/data/reopen.jpg')
    await closeNativeResource('/data/reopen.jpg')
    invoke.mockClear()
    await nativeResourceUrl('/data/reopen.jpg')
    expect(invoke).toHaveBeenCalledWith('native_resource_open', { path: '/data/reopen.jpg' })
  })

  it('does nothing for a path that was never opened', async () => {
    await closeNativeResource('/data/never.jpg')
    expect(invoke).not.toHaveBeenCalled()
  })
})

describe('isNativeResourceUrl', () => {
  it('recognizes the native scheme only', () => {
    expect(isNativeResourceUrl('plain-resource://file/abc')).toBe(true)
    expect(isNativeResourceUrl('http://localhost:8080/fs?id=abc')).toBe(false)
    expect(isNativeResourceUrl('blob:http://localhost/abc')).toBe(false)
  })
})

describe('isExternalReference', () => {
  it('treats web artwork as an external link', () => {
    expect(isExternalReference('https://cdn.example.com/a.jpg')).toBe(true)
    expect(isExternalReference('http://cdn.example.com/a.jpg')).toBe(true)
  })

  it('treats every core reference as the core owns it', () => {
    expect(isExternalReference('fid:abc123.jpg')).toBe(false)
    expect(isExternalReference('app://photos/a.jpg')).toBe(false)
    expect(isExternalReference('/data/a.jpg')).toBe(false)
  })
})
