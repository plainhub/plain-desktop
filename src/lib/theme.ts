import emitter from '@/plugins/eventbus.js'
import { get as prefsGet, set as prefsSet } from '@/lib/prefs/appearance'

export type ColorMode = 'light' | 'dark' | 'auto'

export function isModeDark(mode: ColorMode): boolean {
  return mode === 'auto' ? window.matchMedia('(prefers-color-scheme: dark)').matches : mode === 'dark'
}

export function getCurrentMode(): ColorMode {
  const value = prefsGet('dark_theme', 'USE_DEVICE_THEME')
  return value === 'ON' ? 'dark' : value === 'OFF' ? 'light' : 'auto'
}

export function saveColorMode(mode: ColorMode) {
  void prefsSet('dark_theme', mode === 'dark' ? 'ON' : mode === 'light' ? 'OFF' : 'USE_DEVICE_THEME').catch(console.error)
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
