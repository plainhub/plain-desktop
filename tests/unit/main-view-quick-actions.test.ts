import { describe, expect, it, vi } from 'vitest'
import { Capability } from '../../src/lib/data'

vi.mock('~icons/material-symbols/format-list-numbered-rounded', () => ({ default: {} }))
vi.mock('~icons/material-symbols/queue-music-rounded', () => ({ default: {} }))
vi.mock('~icons/material-symbols/notifications-outline-rounded', () => ({ default: {} }))
vi.mock('~icons/material-symbols/content-paste', () => ({ default: {} }))
vi.mock('~icons/material-symbols/timer-outline', () => ({ default: {} }))
vi.mock('~icons/lucide/bookmark', () => ({ default: {} }))

import { buildQuickActions } from '../../src/views/main-view-quick-actions'

const notificationVisible = (localMode: boolean, capabilities?: string[]) =>
  buildQuickActions({ hasTasks: false, quick: '', localMode, capabilities }).some(action => action.id === 'notification')

describe('notification quick panel', () => {
  it('keeps paired phone notifications accessible without advertising local system notification support', () => {
    expect(notificationVisible(true, [])).toBe(true)
    expect(notificationVisible(true)).toBe(true)
  })

  it('requires the remote provider capability in the web client', () => {
    expect(notificationVisible(false)).toBe(false)
    expect(notificationVisible(false, [])).toBe(false)
    expect(notificationVisible(false, [Capability.CLIPBOARD])).toBe(false)
    expect(notificationVisible(false, [Capability.NOTIFICATIONS])).toBe(true)
  })
})
