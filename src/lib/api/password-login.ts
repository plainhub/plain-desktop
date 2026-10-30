import { chachaDecrypt, chachaEncrypt } from './crypto'
import type { LoginKeys } from './login-channel'

export function createPasswordEncryption(keys: LoginKeys, passwordHash: string) {
  const passwordKey = new TextEncoder().encode(passwordHash.slice(0, 32))

  return {
    encryptRequest(request: string) {
      const passwordEncrypted = chachaEncrypt(passwordKey, request)
      return keys.encryptRequest(passwordEncrypted)
    },
    decryptResponse(frame: Uint8Array) {
      const passwordEncrypted = keys.decryptResponse(frame)
      return chachaDecrypt(passwordKey, passwordEncrypted)
    },
  }
}
