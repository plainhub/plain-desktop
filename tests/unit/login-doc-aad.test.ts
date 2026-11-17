import { expect, it } from 'vitest'
import { chachaDecrypt } from '@/lib/api/crypto'
import { loginFrameSamples } from '../../../plainapp-app/app/data/api-docs/login-frame-samples'

it('decrypts documented resume requests with dot AAD and rejects zero separators', () => {
  for (const sample of Object.values(loginFrameSamples)) {
    const frame = sample.frames[7]
    const bytes = Buffer.from(frame.map(segment => segment.hex).join(''), 'hex')
    const key = Buffer.from(sample.resumeSampleKey, 'hex')
    const plain = JSON.parse(frame.find(segment => segment.kind === 'ciphertext')!.plaintext!)
    expect(chachaDecrypt(key, bytes, Buffer.from('HTTP_REQUEST.POST./init'))).toBe(plain)
    expect(() => chachaDecrypt(key, bytes, Buffer.from('HTTP_REQUEST\0POST\0/init'))).toThrow()
  }
})
