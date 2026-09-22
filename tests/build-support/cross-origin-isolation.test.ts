import { describe, expect, it } from 'vitest'
import { crossOriginIsolationHeaders, isPotentiallyTrustworthyHost } from '../../build-support/cross-origin-isolation'

describe('isPotentiallyTrustworthyHost', () => {
  it('accepts localhost and loopback hosts, with or without a port', () => {
    expect(isPotentiallyTrustworthyHost('localhost:3000')).toBe(true)
    expect(isPotentiallyTrustworthyHost('localhost')).toBe(true)
    expect(isPotentiallyTrustworthyHost('127.0.0.1:4000')).toBe(true)
    expect(isPotentiallyTrustworthyHost('127.127.1.2')).toBe(true)
    expect(isPotentiallyTrustworthyHost('[::1]:3000')).toBe(true)
    expect(isPotentiallyTrustworthyHost('::1')).toBe(true)
    expect(isPotentiallyTrustworthyHost('app.localhost')).toBe(true)
  })

  it('rejects LAN IPs and remote hosts where browsers discard COOP over plain HTTP', () => {
    expect(isPotentiallyTrustworthyHost('192.168.1.5:3000')).toBe(false)
    expect(isPotentiallyTrustworthyHost('10.0.0.2')).toBe(false)
    expect(isPotentiallyTrustworthyHost('desktop.example.com')).toBe(false)
    expect(isPotentiallyTrustworthyHost('localhost.evil.com:3000')).toBe(false)
    expect(isPotentiallyTrustworthyHost(undefined)).toBe(false)
    expect(isPotentiallyTrustworthyHost('')).toBe(false)
  })

  it('strips the port only for host:port shapes, keeping unbracketed IPv6 intact', () => {
    expect(isPotentiallyTrustworthyHost('LOCALHOST:3000')).toBe(true)
    expect(isPotentiallyTrustworthyHost('[::1]')).toBe(true)
    expect(isPotentiallyTrustworthyHost('[::1]:99999')).toBe(true)
  })
})

describe('crossOriginIsolationHeaders', () => {
  it('keeps the COOP/COEP pair that crossOriginIsolated (WebCodecs) requires', () => {
    expect(crossOriginIsolationHeaders).toEqual({
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'credentialless',
    })
  })
})
