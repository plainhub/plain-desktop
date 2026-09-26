import { describe, expect, it } from 'vitest'
import { deriveLoginChatKey } from '@/lib/device/login-chat-key'

describe('deriveLoginChatKey', () => {
  it('derives a separate chat key from the login token', () => {
    const token = 'AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8='
    expect(deriveLoginChatKey(token)).toBe('C6I48EcKz92pATWNkQcVzJgTSaLJNmuJfdX/al/qY6M=')
    expect(deriveLoginChatKey(token)).not.toBe(token)
  })
})
