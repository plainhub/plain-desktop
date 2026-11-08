import { afterEach, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import { isPermissionGranted, open, openUrl, requestPermission, revealItemInDir, save } from '@/lib/tauri-plugins'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

afterEach(() => {
  vi.resetAllMocks()
  vi.unstubAllGlobals()
})

it('keeps file dialogs and system URL and path actions behind their existing native permissions', async () => {
  const options = { multiple: true as const, filters: [{ name: 'Media', extensions: ['png'] }] }
  vi.mocked(invoke).mockResolvedValueOnce(['/images/one.png']).mockResolvedValueOnce(null)
  await expect(open(options)).resolves.toEqual(['/images/one.png'])
  await expect(save({ defaultPath: 'untitled.txt' })).resolves.toBeNull()
  await openUrl('https://example.test/help')
  await revealItemInDir('/images/one.png')
  expect(invoke).toHaveBeenNthCalledWith(1, 'plugin:dialog|open', { options })
  expect(invoke).toHaveBeenNthCalledWith(2, 'plugin:dialog|save', { options: { defaultPath: 'untitled.txt' } })
  expect(invoke).toHaveBeenNthCalledWith(3, 'plugin:opener|open_url', { url: 'https://example.test/help' })
  expect(invoke).toHaveBeenNthCalledWith(4, 'plugin:opener|reveal_item_in_dir', { paths: ['/images/one.png'] })
  vi.mocked(invoke).mockRejectedValueOnce(new Error('Denied'))
  await expect(openUrl('https://example.test/help')).rejects.toThrow('Denied')
})

it('uses the existing notification permission query and WebView permission request', async () => {
  const notification = { permission: 'default', requestPermission: vi.fn().mockResolvedValue('granted') }
  vi.stubGlobal('window', { Notification: notification })
  vi.mocked(invoke).mockResolvedValueOnce(true)
  await expect(isPermissionGranted()).resolves.toBe(true)
  expect(invoke).toHaveBeenCalledExactlyOnceWith('plugin:notification|is_permission_granted')
  await expect(requestPermission()).resolves.toBe('granted')
  expect(notification.requestPermission).toHaveBeenCalledTimes(1)
  notification.permission = 'denied'
  await expect(isPermissionGranted()).resolves.toBe(false)
  notification.permission = 'granted'
  await expect(isPermissionGranted()).resolves.toBe(true)
  expect(invoke).toHaveBeenCalledTimes(1)
})
