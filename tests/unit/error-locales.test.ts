import { createI18n } from 'vue-i18n'
import { expect, it } from 'vitest'
import { AuthErrorCode } from '@/lib/api/auth-error-code'
import { loginErrorMessage } from '@/lib/api/login-error'
import { PairingErrorCode, pairingErrorMessage } from '@/lib/api/pairing-error-code'
import { AccessErrorCode } from '@/lib/api/access-error-code'

const locales = import.meta.glob<{ default: Record<string, string> }>('../../src/locales/*/index.ts')
const keys = [AccessErrorCode.NoPermission, ...Object.values(AuthErrorCode).map(loginErrorMessage), ...Object.values(PairingErrorCode).map(pairingErrorMessage)]

it.each(Object.entries(locales))('translates permission, login and pairing errors in %s', async (path, load) => {
  const locale = path.split('/').at(-2)!
  const { default: messages } = await load()
  const { t } = createI18n({ legacy: false, locale, fallbackLocale: false, messages: { [locale]: messages } }).global
  for (const key of keys) expect(t(key), key).not.toBe(key)
})
