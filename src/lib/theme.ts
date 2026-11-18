import emitter from '@/plugins/eventbus.js'
import { get as prefsGet, set as prefsSet } from '@/lib/prefs/appearance'

export type ColorMode = ReturnType<typeof prefsGet<'dark_theme'>>

export function isModeDark(mode: ColorMode): boolean {
  return mode === 'USE_DEVICE_THEME' ? window.matchMedia('(prefers-color-scheme: dark)').matches : mode === 'ON'
}

export function getCurrentMode(): ColorMode {
  const value = prefsGet('dark_theme', 'USE_DEVICE_THEME')
  return value === 'ON' || value === 'OFF' ? value : 'USE_DEVICE_THEME'
}

export function saveColorMode(mode: ColorMode) {
  void prefsSet('dark_theme', mode).catch(console.error)
}

export function changeColor() {
  const lastColorMode = getCurrentMode()
  const isDark = isModeDark(lastColorMode)
  applyDarkClass(isDark)
}

export function changeColorMode(mode: ColorMode) {
  const isDark = isModeDark(mode)
  saveColorMode(mode)
  applyDarkClass(isDark)
}

export function applyDarkClass(isDark?: boolean) {
  if (isDark) {
    document.documentElement.classList.add('dark')
  } else {
    document.documentElement.classList.remove('dark')
  }
  emitter.emit('color_mode_changed')
}
