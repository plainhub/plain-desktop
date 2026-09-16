import { describe, it, expect, vi, afterEach } from 'vitest'
import { getApiBaseUrl, getWebSocketBaseUrl } from '@/lib/api/api'

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
  it('routes the event socket to /ws with wss on a *43 TLS port', () => {
    vi.stubEnv('VITE_APP_API_HOST', 'smartbox:8443')
    expect(getWebSocketBaseUrl()).toBe('wss://smartbox:8443/ws')
  })

  it('routes the event socket to /ws with ws on a plain port', () => {
    vi.stubEnv('VITE_APP_API_HOST', '192.168.123.20:8080')
    expect(getWebSocketBaseUrl()).toBe('ws://192.168.123.20:8080/ws')
  })
})
