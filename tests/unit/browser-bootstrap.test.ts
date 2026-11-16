import { afterEach, expect, it, vi } from 'vitest'

const io = vi.hoisted(() => ({
  ensureBrowserClientId: vi.fn().mockReturnValue(false),
  applyUrlClientId: vi.fn(),
  mount: vi.fn(),
}))

vi.mock('vue', () => ({
  createApp: () => {
    const app = { use: () => app, provide: () => app, mount: io.mount }
    return app
  },
}))
vi.mock('@/App.vue', () => ({ default: {} }))
vi.mock('@/plugins/router', () => ({ default: {} }))
vi.mock('@/styles/main.scss', () => ({}))
vi.mock('pinia', () => ({ createPinia: () => ({}) }))
vi.mock('@/plugins/clickaway', () => ({ default: {} }))
vi.mock('@/plugins/tooltip', () => ({ default: {} }))
vi.mock('@/plugins/ripple', () => ({ default: {} }))
vi.mock('@/hooks/device', () => ({ getIsPhone: () => false, getIsTablet: () => false }))
vi.mock('@/lib/api/api', () => ({}))
vi.mock('@/lib/prefs/core', () => ({}))
vi.mock('@/lib/device/client-id', () => io)
vi.mock('@/lib/device/login-peers', () => ({}))
vi.mock('@/lib/app-menu', () => ({}))
vi.mock('@/lib/file-drop', () => ({}))
vi.mock('@/lib/api/local-server', () => ({}))
vi.mock('@/lib/device/local-mode', () => ({}))
vi.mock('@/plugins/i18n', () => ({ default: {}, syncLocaleFromPrefs: () => 'en-US', loadLocaleMessages: vi.fn() }))
vi.mock('@/lib/timeago', () => ({ setLocale: vi.fn() }))

import { bootstrapFullApp } from '@/bootstrap/full-app'

afterEach(() => vi.unstubAllGlobals())

it('mounts the router at the original URL after initializing a browser identity', async () => {
  const replace = vi.fn()
  vi.stubGlobal('__IS_TAURI__', false)
  vi.stubGlobal('window', { location: { pathname: '/text-file', replace } })

  await bootstrapFullApp()

  expect(io.ensureBrowserClientId).toHaveBeenCalledOnce()
  expect(io.applyUrlClientId).toHaveBeenCalledOnce()
  expect(io.mount).toHaveBeenCalledExactlyOnceWith('#app')
  expect(replace).not.toHaveBeenCalled()
})
