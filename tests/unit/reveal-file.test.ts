import { afterEach, expect, it, vi } from 'vitest'
import { useRevealFile } from '@/views/chat/hooks/reveal-file'

const target = vi.hoisted(() => ({ remote: '' }))
const actions = vi.hoisted(() => ({
  revealLocalFile: vi.fn().mockResolvedValue(undefined),
  saveLocalFileAs: vi.fn().mockResolvedValue(undefined),
  copyLocalFile: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('@/lib/device/client-id', () => ({ getRemoteClientId: () => target.remote }))
vi.mock('@/lib/native-file-actions', () => actions)
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

afterEach(() => {
  vi.unstubAllGlobals()
  vi.clearAllMocks()
})

it.each([[false, ''], [false, 'peer'], [true, 'peer']])('omits host file actions outside desktop local mode (%s, %s)', (desktop, remote) => {
  vi.stubGlobal('__IS_TAURI__', desktop)
  target.remote = remote
  expect(useRevealFile().buildMediaMenuItems('/documents/report.txt')).toEqual([])
})

it('provides all host file actions in desktop local mode without an app capability query', async () => {
  vi.stubGlobal('__IS_TAURI__', true)
  target.remote = ''
  const menu = useRevealFile().buildMediaMenuItems('/documents/report.txt', 'report.txt')
  expect(menu.map(item => item.label)).toEqual(['show_in_finder', 'save_as', 'copy'])
  for (const item of menu) await item.onClick?.()
  expect(actions.revealLocalFile).toHaveBeenCalledExactlyOnceWith('/documents/report.txt')
  expect(actions.saveLocalFileAs).toHaveBeenCalledExactlyOnceWith('/documents/report.txt', 'report.txt')
  expect(actions.copyLocalFile).toHaveBeenCalledExactlyOnceWith('/documents/report.txt')
})

it.each(['', 'fsid:external-resource'])('omits file actions without a usable local reference (%s)', path => {
  vi.stubGlobal('__IS_TAURI__', true)
  target.remote = ''
  expect(useRevealFile().buildMediaMenuItems(path)).toEqual([])
})
