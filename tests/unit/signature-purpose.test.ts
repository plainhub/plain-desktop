import { execFileSync } from 'node:child_process'
import { expect, it } from 'vitest'
import { SignaturePurpose } from '../../src/lib/api/signature-purpose.generated'

it('uses the generated Rust signature purposes without duplicate domains', () => {
  execFileSync(process.execPath, [new URL('../../scripts/signature-purpose-codegen.mjs', import.meta.url).pathname])
  const values = Object.values(SignaturePurpose)
  expect(new Set(values).size).toBe(values.length)
  expect(values.sort()).toEqual([
    'CHANNEL_MEMBERSHIP',
    'CHAT_PAIR_REQUEST',
    'PAIRING_ACCEPT',
    'PAIRING_REJECT',
    'PAIR_CANCEL',
    'PEER_REQUEST',
    'PEER_STATUS',
    'PEER_TLS_CERTIFICATE',
    'QR_PAIR_ACCEPT',
    'WEB_APPROVAL_RESPONSE',
  ].sort())
})
