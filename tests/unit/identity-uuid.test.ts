import { afterEach, describe, expect, it, vi } from 'vitest'
import { generateClientId } from '@/lib/device/client-identity.generated'
import { generateRecordId } from '@/lib/record-id.generated'

afterEach(() => vi.restoreAllMocks())

describe('shared identity layouts', () => {
  it('uses the Plain UUIDv8 prefix, version and variant for browser clients', () => {
    const ids = new Set(Array.from({ length: 256 }, () => generateClientId()))
    expect(ids.size).toBe(256)
    for (const id of ids) {
      expect(id).toMatch(/^706c6169-[0-9a-f]{4}-8[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
    }
  })

  it('rerolls a candidate in the legacy device ID range', () => {
    const random = vi.spyOn(crypto, 'getRandomValues')
      .mockImplementationOnce((bytes) => {
        if (bytes instanceof Uint8Array) bytes.set([0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0x30, 0x39])
        return bytes
      })
      .mockImplementationOnce((bytes) => {
        if (bytes instanceof Uint8Array) {
          bytes.fill(0)
          bytes[5] = 2
        }
        return bytes
      })

    expect(generateClientId()).toBe('706c6169-0002-8000-8000-000000000000')
    expect(random).toHaveBeenCalledTimes(2)
  })

  it('stores Unix milliseconds in UUIDv7 database record IDs', () => {
    const millis = 1793836800123
    vi.spyOn(Date, 'now').mockReturnValue(millis)
    const first = generateRecordId()
    expect(first).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
    expect(Number.parseInt(first.replaceAll('-', '').slice(0, 12), 16)).toBe(millis)
    expect(generateRecordId()).not.toBe(first)
    vi.mocked(Date.now).mockReturnValue(millis + 1)
    expect(generateRecordId() > first).toBe(true)
  })
})
