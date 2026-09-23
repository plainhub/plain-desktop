import type { Component } from 'vue'
import { Capability } from '@/lib/data'
import { hasFeature } from '@/lib/feature'
import IMaterialSymbolsFormatListNumbered from '~icons/material-symbols/format-list-numbered-rounded'
import IMaterialSymbolsQueueMusicRounded from '~icons/material-symbols/queue-music-rounded'
import IMaterialSymbolsNotificationsOutlineRounded from '~icons/material-symbols/notifications-outline-rounded'
import IMaterialSymbolsContentPaste from '~icons/material-symbols/content-paste'
import IMaterialSymbolsTimerOutline from '~icons/material-symbols/timer-outline'
import ILucideBookmark from '~icons/lucide/bookmark'

export interface QuickAction {
  id: string
  tooltipKey: string
  icon: Component
  visible: boolean
}

export interface QuickActionsInput {
  /** Desktop acting as the local NAS server (no phone attached). */
  localMode: boolean
  hasTasks: boolean
  /** Currently open quick panel; keeps upload visible while selected. */
  quick: string
  /** Server-declared App.capabilities; undefined until the app query resolves. */
  capabilities?: string[]
}

/** Single source of truth for the right-rail quick actions and their gating.
 *  Every id returned here MUST have a panel mounted in MainView's
 *  quick-content (locked by tests/views/main-view-quick-panels.test.ts) —
 *  the 2026-09-16 NAS trim deleted the mounts and orphaned these panels.
 *  Mirror panels (notification/clipboard/pomodoro) are gated on the connected
 *  device's declared capability alone: phones declare all three; the local
 *  desktop server declares NOTIFICATIONS (peer aggregation) but not
 *  CLIPBOARD/POMODORO. */
export function buildQuickActions({ localMode, hasTasks, quick, capabilities }: QuickActionsInput): QuickAction[] {
  return [
    { id: 'upload', tooltipKey: 'header_actions.uploads', icon: IMaterialSymbolsFormatListNumbered, visible: !localMode && (hasTasks || quick === 'upload') },
    { id: 'notification', tooltipKey: 'header_actions.notifications', icon: IMaterialSymbolsNotificationsOutlineRounded, visible: hasFeature(Capability.NOTIFICATIONS, capabilities) },
    { id: 'clipboard', tooltipKey: 'header_actions.clipboard', icon: IMaterialSymbolsContentPaste, visible: !localMode && hasFeature(Capability.CLIPBOARD, capabilities) },
    { id: 'audio', tooltipKey: 'playlist', icon: IMaterialSymbolsQueueMusicRounded, visible: !localMode },
    { id: 'pomodoro', tooltipKey: 'pomodoro_timer', icon: IMaterialSymbolsTimerOutline, visible: !localMode && hasFeature(Capability.POMODORO, capabilities) },
    { id: 'bookmark', tooltipKey: 'bookmarks', icon: ILucideBookmark, visible: true },
  ].filter((action) => action.visible)
}
