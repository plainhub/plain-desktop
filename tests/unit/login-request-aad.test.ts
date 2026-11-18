import { expect, it } from 'vitest'
import { chachaDecrypt, httpRequestAad } from '@/lib/api/crypto'

it('decrypts a fixed session resume request with dot AAD and rejects NUL separators', () => {
  const key = Buffer.from('000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f', 'hex')
  // Fixed libsodium XChaCha20-Poly1305 vector: nonce || ciphertext || tag.
  const frame = Buffer.from(
    '202122232425262728292a2b2c2d2e2f3031323334353637' +
    '2c6e7dfb4b0724bb088e618f7b3b31e10c64e30eeca03281d4546a1f380ed23a08784123a381456d36c0502bdbf74a6d07bad93d20daac97cf865d3045e5ad61e1d31d',
    'hex',
  )

  expect(chachaDecrypt(key, frame, httpRequestAad('/init'))).toBe('1700000000123|00112233445566778899aabbccddeeff|init')
  expect(() => chachaDecrypt(key, frame, Buffer.from('HTTP_REQUEST\0POST\0/init'))).toThrow()
})
