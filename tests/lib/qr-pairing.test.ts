import { describe, expect, it } from 'vitest'
import { buildQrPairPayload, qrPairedDevice } from '@/lib/device/qr-pairing'
import type { PairingRequest } from '@/lib/pairing-types'

describe('qr-pairing payload', () => {
  it('encodes identity, all ips and the https port', () => {
    const payload = buildQrPairPayload(
      { clientId: 'abc123', deviceName: 'Mac Book Pro' },
      ['192.168.123.22', '198.19.0.33'],
      8443,
    )
    expect(payload).toBe(
      'plainapp://pair?v=1&id=abc123&name=Mac%20Book%20Pro&ips=192.168.123.22,198.19.0.33&port=8443',
    )
  })

  it('percent-encodes non-ascii device names', () => {
    const payload = buildQrPairPayload({ clientId: 'id', deviceName: '中文' }, [], 1)
    expect(payload).toContain('name=%E4%B8%AD%E6%96%87')
  })

  it('keeps the version query first so unknown params can be added later', () => {
    const payload = buildQrPairPayload({ clientId: 'x', deviceName: 'n' }, [], 1)
    expect(payload.startsWith('plainapp://pair?v=1&')).toBe(true)
  })
})

describe('qrPairedDevice', () => {
  const base = {
    ecdhPublicKey: '',
    signaturePublicKey: '',
    timestamp: 0,
    signature: '',
  } as const

  it('prefers the stamped sender ip for the login host', () => {
    const request: PairingRequest = {
      ...base,
      fromId: 'phone1',
      fromName: 'Pixel 7',
      fromIp: '192.168.123.21',
      port: 8443,
      deviceType: 'PHONE',
      ips: ['10.0.0.1'],
    }
    const device = qrPairedDevice(request)
    expect(device).toMatchObject({
      id: 'phone1',
      name: 'Pixel 7',
      ip: '192.168.123.21',
      port: 8443,
      host: '192.168.123.21:8443',
      deviceType: 'PHONE',
    })
  })

  it('falls back to the announced ips and a PHONE device type', () => {
    const request: PairingRequest = {
      ...base,
      fromId: 'phone2',
      fromName: 'S20',
      port: 9000,
      deviceType: '',
      ips: ['192.168.1.37'],
    }
    const device = qrPairedDevice(request)
    expect(device.host).toBe('192.168.1.37:9000')
    expect(device.deviceType).toBe('PHONE')
  })

  it('keeps host empty when no address is known', () => {
    const request: PairingRequest = { ...base, fromId: 'p', fromName: 'x', port: 1, deviceType: '', ips: [] }
    expect(qrPairedDevice(request).host).toBe('')
  })
})
