import { afterEach, expect, it, vi } from 'vitest'
vi.mock('@/lib/api/http', () => ({ httpRequest: vi.fn() }))
import { httpRequest } from '../../src/lib/api/http'
import { sha512 } from '../../src/lib/api/crypto'
import { secureSetupUrl, setupPassword } from '../../src/lib/api/password-setup'

afterEach(() => vi.resetAllMocks())

it('upgrades the setup page to the actual TLS port and preserves navigation', () => {
  expect(secureSetupUrl('http://device.example:8080/setup?redirect=%2Ffiles#form', 8443)).toBe('https://device.example:8443/setup?redirect=%2Ffiles#form')
  expect(() => secureSetupUrl('http://device.example/setup', 0)).toThrow()
  expect(() => secureSetupUrl('http://device.example/setup', undefined as unknown as number)).toThrow()
})

it('never sends a password over HTTP', async () => {
  await expect(setupPassword('http://device.example:8080', 'browser', 'owned-password')).rejects.toThrow('HTTPS')
  expect(httpRequest).not.toHaveBeenCalled()
})

it('sends only the password digest over TLS without knowing the server salt', async () => {
  vi.mocked(httpRequest).mockResolvedValue(new Response(null, { status: 204 }))
  await setupPassword('https://device.example:8443', 'browser', 'owned-password')
  const [url, options] = vi.mocked(httpRequest).mock.calls[0]!
  expect(url).toBe('https://device.example:8443/auth/setup')
  expect(options?.headers).toEqual({ 'c-id': 'browser', 'Content-Type': 'application/json' })
  expect(JSON.parse(new TextDecoder().decode(options?.body!))).toEqual({ passwordHash: sha512('owned-password') })
})

it('reports replacement rejection and transport failures', async () => {
  vi.mocked(httpRequest).mockResolvedValue(new Response(null, { status: 409 }))
  await expect(setupPassword('https://device.example', 'browser', 'replacement')).rejects.toThrow()
  vi.mocked(httpRequest).mockRejectedValue(new Error('offline'))
  await expect(setupPassword('https://device.example', 'browser', 'owned-password')).rejects.toThrow('offline')
})
