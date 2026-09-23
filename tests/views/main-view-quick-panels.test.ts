import { describe, expect, it } from 'vitest'
import mainViewSource from '../../src/views/MainView.vue?raw'
import { buildQuickActions } from '@/views/main-view-quick-actions'

/** Source contract: every quick action id exposed by buildQuickActions must
 *  have a panel mounted in MainView's quick-content. The 2026-09-16 NAS-mode
 *  trim (696e6828) deleted the notification/clipboard/pomodoro mounts while
 *  their components and data layers stayed alive — panels went silent and
 *  nothing went red. This file locks the mounts so that cannot recur. */

const ALL_IDS = buildQuickActions({
  localMode: false,
  hasTasks: false,
  quick: '',
  capabilities: ['NOTIFICATIONS', 'CLIPBOARD', 'POMODORO'],
}).map((a) => a.id)

const PANEL_MOUNTS: Record<string, { component: string; condition: string }> = {
  upload: { component: 'upload-list', condition: 'store.quick === \'upload\'' },
  notification: { component: 'p-notifications', condition: 'store.quick === \'notification\'' },
  clipboard: { component: 'p-clipboard', condition: 'store.quick === \'clipboard\'' },
  audio: { component: 'audio-player', condition: 'store.quick === \'audio\'' },
  pomodoro: { component: 'pomodoro-timer', condition: 'store.quick === \'pomodoro\'' },
  bookmark: { component: 'bookmark-list', condition: 'store.quick === \'bookmark\'' },
}

const mountLine = (m: { component: string; condition: string }) =>
  new RegExp(`<${m.component}[\\s\\S]*?v-show="${m.condition.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}"`)

describe('MainView quick panel mounts (source contract)', () => {
  it('every quick action id has a matching mounted panel', () => {
    for (const id of ALL_IDS) {
      const mount = PANEL_MOUNTS[id]
      expect(mount, `no panel mount declared for quick action '${id}'`).toBeTruthy()
      expect(mainViewSource, `quick panel '${id}' (${mount!.component}) is not mounted in MainView`).toMatch(mountLine(mount!))
    }
  })

  it('phone-mirror panels are capability-gated, not channel- or mode-gated', () => {
    expect(mainViewSource).toMatch(/<p-notifications v-if="!localMode && hasNotifications" v-show="store\.quick === 'notification'"/)
    expect(mainViewSource).toMatch(/<local-notifications v-if="localMode && hasNotifications" v-show="store\.quick === 'notification'"/)
    expect(mainViewSource).toMatch(/<p-clipboard v-if="hasClipboard" v-show="store\.quick === 'clipboard'"/)
    expect(mainViewSource).toMatch(/<pomodoro-timer v-if="hasPomodoro" v-show="store\.quick === 'pomodoro'"/)
    expect(mainViewSource).not.toMatch(/buildChannel[^>]*notification|notification[^>]*buildChannel/)
  })

  it('quick actions are built by the shared builder, not an inline list', () => {
    expect(mainViewSource).toMatch(/quickActions = computed\(\(\) =>\s*\n\s*buildQuickActions\(/)
  })

  it('panels live inside the quick-content area that shows while store.quick is set', () => {
    const contentStart = mainViewSource.indexOf('class="quick-content"')
    const contentEnd = mainViewSource.indexOf('</transition>', contentStart)
    expect(contentStart).toBeGreaterThan(-1)
    for (const id of ALL_IDS) {
      const mount = PANEL_MOUNTS[id]!
      const at = mainViewSource.indexOf(`<${mount.component}`, contentStart)
      expect(at, `${mount.component} is mounted outside the quick-content area`).toBeGreaterThan(-1)
      expect(at).toBeLessThan(contentEnd)
    }
  })
})
