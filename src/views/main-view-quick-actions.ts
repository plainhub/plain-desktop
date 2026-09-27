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
  hasTasks: boolean
  /** Currently open quick panel; keeps upload visible while selected. */
  quick: string
  /** Server-declared App.capabilities; undefined until the app query resolves. */
  capabilities?: string[]
}

export function buildQuickActions({ hasTasks, quick, capabilities }: QuickActionsInput): QuickAction[] {
  return [
    { id: 'upload', tooltipKey: 'header_actions.uploads', icon: IMaterialSymbolsFormatListNumbered, visible: hasTasks || quick === 'upload' },
    { id: 'notification', tooltipKey: 'header_actions.notifications', icon: IMaterialSymbolsNotificationsOutlineRounded, visible: hasFeature(Capability.NOTIFICATIONS, capabilities) },
    { id: 'clipboard', tooltipKey: 'header_actions.clipboard', icon: IMaterialSymbolsContentPaste, visible: hasFeature(Capability.CLIPBOARD, capabilities) },
    { id: 'audio', tooltipKey: 'playlist', icon: IMaterialSymbolsQueueMusicRounded, visible: true },
    { id: 'pomodoro', tooltipKey: 'pomodoro_timer', icon: IMaterialSymbolsTimerOutline, visible: hasFeature(Capability.POMODORO, capabilities) },
    { id: 'bookmark', tooltipKey: 'bookmarks', icon: ILucideBookmark, visible: true },
  ].filter((action) => action.visible)
}
