import * as browser from './browser'
import * as core from './core'

interface AppearancePreferences {
  locale: string
  dark_theme: 'USE_DEVICE_THEME' | 'ON' | 'OFF'
}

export const APPEARANCE_KEYS = ['locale', 'dark_theme'] as const

export function get<K extends keyof AppearancePreferences>(key: K, fallback: AppearancePreferences[K]): AppearancePreferences[K] {
  return __IS_TAURI__ ? core.get(key, fallback) : browser.get(key, fallback)
}

export function set<K extends keyof AppearancePreferences>(key: K, value: AppearancePreferences[K]): Promise<void> {
  if (__IS_TAURI__) return core.set(key, value)
  browser.set(key, value)
  return Promise.resolve()
}

export async function apply(): Promise<void> {
  const [{ changeColor }, { loadLocaleMessages, syncLocaleFromPrefs }] = await Promise.all([
    import('@/lib/theme'), import('@/plugins/i18n'),
  ])
  changeColor()
  const locale = get('locale', '')
  if (locale) await loadLocaleMessages(locale)
  syncLocaleFromPrefs()
}
