import { expect, it } from 'vitest'
import { httpsPageUrl, isHttpBrowser } from '../../src/lib/http-security'

it.each([
  ['http:', false, true],
  ['https:', false, false],
  ['http:', true, false],
  ['tauri:', true, false],
  ['file:', false, false],
])('shows the warning for %s with native=%s: %s', (protocol, nativeApp, expected) => {
  expect(isHttpBrowser(protocol, nativeApp)).toBe(expected)
})

it.each([undefined, null, 0, -1, 65536, 443.5, '443', NaN, Infinity])('rejects unavailable or invalid HTTPS port %s', port => {
  expect(httpsPageUrl('http://example.test/files', port)).toBe('')
})

it.each(['example.test', '127.0.0.1', '[::1]'])('preserves the route for host %s using the actual port', host => {
  expect(httpsPageUrl(`http://${host}:8080/files?path=%2Fphotos#details`, 9443))
    .toBe(`https://${host}:9443/files?path=%2Fphotos#details`)
})

it('supports standard and boundary ports and ignores HTTPS pages', () => {
  expect(httpsPageUrl('http://example.test/', 443)).toBe('https://example.test/')
  expect(httpsPageUrl('http://example.test/', 1)).toBe('https://example.test:1/')
  expect(httpsPageUrl('http://example.test/', 65535)).toBe('https://example.test:65535/')
  expect(httpsPageUrl('https://example.test/', 9443)).toBe('')
})

it('preserves shared-link route state without copying URL login credentials', () => {
  expect(httpsPageUrl('http://user:pass@example/s/id?path=photos#share-token', 9443))
    .toBe('https://example:9443/s/id?path=photos#share-token')
})
