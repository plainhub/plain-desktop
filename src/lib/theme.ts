import emitter from '@/plugins/eventbus.js'
import { get as prefsGet, set as prefsSet } from '@/lib/prefs'

export type ColorMode = 'light' | 'dark' | 'auto'

export function isModeDark(mode: ColorMode, saveAutoMode = true) {
  let isDark = mode === 'dark'

  // Determines whether the auto mode should display light or dark.
  if (mode === 'auto') {
    isDark = window.matchMedia('(prefers-color-scheme: dark)').matches
    if (saveAutoMode) {
      // We have to save this because if the user closes the tab when it's light
      // and reopens it when it's dark, we need to know whether the last applied
      // 'auto' mode was correct.
      saveLastSavedAutoColorMode(isDark ? 'dark' : 'light')
    }
  }

  return isDark
}


export function getCurrentMode(): ColorMode | null {
  const value = prefsGet<number>('dark_theme', 0)
  return value === 1 ? 'dark' : value === 2 ? 'light' : 'auto'
}

export function saveColorMode(mode: ColorMode) {
  void prefsSet('dark_theme', mode === 'dark' ? 1 : mode === 'light' ? 2 : 0).catch(console.error)
}

export function getLastSavedAutoColorMode() {
  return prefsGet<'light' | 'dark' | null>('last-auto-color-mode', null)
}

export function saveLastSavedAutoColorMode(mode: 'light' | 'dark') {
  prefsSet('last-auto-color-mode', mode)
}

export function changeColor() {
  const lastColorMode = getCurrentMode()!
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
