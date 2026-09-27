import { describe, expect, it } from 'vitest'
import { buildQuickActions } from '@/views/main-view-quick-actions'
import { Capability } from '@/lib/data'

const PHONE_CAPABILITIES = Object.values(Capability)
/** The desktop local server declares these (see src-tauri local app query):
 *  NOTIFICATIONS because the resident peer layer aggregates logged-in phones. */
const LOCAL_CAPABILITIES = ['DOC_PREVIEW', 'IMAGE_EDITOR', 'NOTIFICATIONS']

const base = { hasTasks: false, quick: '' }

describe('buildQuickActions', () => {
  it('remote phone: uploads, notifications, clipboard, audio, pomodoro, bookmarks — in order', () => {
    const ids = buildQuickActions({ ...base, hasTasks: true, capabilities: PHONE_CAPABILITIES }).map((a) => a.id)
    expect(ids).toEqual(['upload', 'notification', 'clipboard', 'audio', 'pomodoro', 'bookmark'])
  })

  it('local mode keeps shared upload and audio panels', () => {
    const ids = buildQuickActions({ ...base, hasTasks: true, capabilities: LOCAL_CAPABILITIES }).map((a) => a.id)
    expect(ids).toEqual(['upload', 'notification', 'audio', 'bookmark'])
  })

  it('notification/clipboard/pomodoro entries are gated on the connected device capabilities', () => {
    const actions = buildQuickActions({ ...base, capabilities: ['NOTIFICATIONS', 'CLIPBOARD', 'POMODORO'] })
    expect(actions.map((a) => a.id)).toEqual(['notification', 'clipboard', 'audio', 'pomodoro', 'bookmark'])
    const none = buildQuickActions({ ...base, capabilities: [] }).map((a) => a.id)
    expect(none).not.toContain('notification')
    expect(none).not.toContain('clipboard')
    expect(none).not.toContain('pomodoro')
  })

  it('mirror entries stay hidden until the app query resolves capabilities', () => {
    const ids = buildQuickActions({ ...base }).map((a) => a.id)
    expect(ids).not.toContain('notification')
    expect(ids).not.toContain('clipboard')
    expect(ids).not.toContain('pomodoro')
  })

  it('bookmarks are always available in both modes', () => {
    const ids = buildQuickActions({ ...base, capabilities: [] }).map((a) => a.id)
    expect(ids).toContain('bookmark')
  })

  it('upload stays visible while its panel is selected even with no active tasks', () => {
    const idle = buildQuickActions({ hasTasks: false, quick: '', capabilities: [] })
    expect(idle.some((a) => a.id === 'upload')).toBe(false)
    const selected = buildQuickActions({ hasTasks: false, quick: 'upload', capabilities: [] })
    expect(selected.some((a) => a.id === 'upload')).toBe(true)
  })
})
