import { describe, it, expect, vi, afterEach } from 'vitest'
import { buildProxyHttpUrl, buildProxyWsUrl, getApiBaseUrl, getWebSocketBaseUrl } from '@/lib/api/api'

afterEach(() => {
  vi.unstubAllEnvs()
})

describe('getApiBaseUrl', () => {
  it('upgrades to https when the API host is on a *43 TLS port', () => {
    vi.stubEnv('VITE_APP_API_HOST', 'smartbox:8443')
    expect(getApiBaseUrl()).toBe('https://smartbox:8443')
  })

  it('stays on http for a plain port', () => {
    vi.stubEnv('VITE_APP_API_HOST', '192.168.123.20:8080')
    expect(getApiBaseUrl()).toBe('http://192.168.123.20:8080')
  })

  it('stays http when neither the page nor the API port is secure', () => {
    vi.stubEnv('VITE_APP_API_HOST', 'smartbox:8080')
    expect(getApiBaseUrl()).toBe('http://smartbox:8080')
  })

  it('falls back to the page host when no API host is configured', () => {
    vi.stubEnv('VITE_APP_API_HOST', '')
    const base = getApiBaseUrl()
    expect(base).toMatch(/^http:\/\/localhost:\d+$/)
  })
})

describe('getWebSocketBaseUrl', () => {
  it('keeps the socket at `/` with wss on a *43 TLS port', () => {
    vi.stubEnv('VITE_APP_API_HOST', 'smartbox:8443')
    expect(getWebSocketBaseUrl()).toBe('wss://smartbox:8443')
  })

  it('keeps the socket at `/` with ws on a plain port', () => {
    vi.stubEnv('VITE_APP_API_HOST', '198.51.100.20:8080')
    expect(getWebSocketBaseUrl()).toBe('ws://198.51.100.20:8080')
  })
})

describe('buildProxyHttpUrl', () => {
  it('rewrites an https device URL onto the loopback proxy with _pt', () => {
    expect(buildProxyHttpUrl(9091, 'https://phone.example:8443/graphql')).toBe(
      `http://127.0.0.1:9091/graphql?_pt=${encodeURIComponent('https://phone.example:8443')}`,
    )
  })

  it('preserves existing query params and appends _pt after them', () => {
    const url = 'https://phone.example:8443/fs?id=abc%2Bdef&w=50'
    expect(buildProxyHttpUrl(9091, url)).toBe(
      `http://127.0.0.1:9091/fs?id=abc%2Bdef&w=50&_pt=${encodeURIComponent('https://phone.example:8443')}`,
    )
  })

  it('uses / when the URL has no path', () => {
    expect(buildProxyHttpUrl(9091, 'https://phone.example:8443')).toBe(
      `http://127.0.0.1:9091/?_pt=${encodeURIComponent('https://phone.example:8443')}`,
    )
  })
})

describe('buildProxyWsUrl', () => {
  it('rewrites a device wss URL with _pt and _cid, keeping other params', () => {
    expect(buildProxyWsUrl(9091, 'wss://phone.example:8443/?cid=abc', 'peer-1')).toBe(
      `ws://127.0.0.1:9091/?cid=abc&_pt=${encodeURIComponent('wss://phone.example:8443')}&_cid=peer-1`,
    )
  })

  it('keeps the ws scheme for plain-http devices and omits _cid when empty', () => {
    expect(buildProxyWsUrl(9091, 'ws://phone.example:8080')).toBe(
      `ws://127.0.0.1:9091/?_pt=${encodeURIComponent('ws://phone.example:8080')}`,
    )
  })

  it('passes loopback sockets through unchanged (local server stays direct)', () => {
    expect(buildProxyWsUrl(9091, 'ws://localhost:8080/?cid=x')).toBe('ws://localhost:8080/?cid=x')
    expect(buildProxyWsUrl(9091, 'ws://127.0.0.1:8080')).toBe('ws://127.0.0.1:8080')
  })

  it('normalizes a missing path to /', () => {
    expect(buildProxyWsUrl(9091, 'wss://phone.example:8443?cid=abc', 'peer-1')).toBe(
      `ws://127.0.0.1:9091/?cid=abc&_pt=${encodeURIComponent('wss://phone.example:8443')}&_cid=peer-1`,
    )
  })
})
