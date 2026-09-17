import type { Component } from 'vue'
import { AppChannelType, DeviceType } from '@/lib/status'
import ILucideFolder from '~icons/lucide/folder'
import ILucideMusic from '~icons/lucide/music'
import ILucideImage from '~icons/lucide/image'
import ILucideVideo from '~icons/lucide/video'
import ILucideMessageCircle from '~icons/lucide/message-circle'
import ILucideFileText from '~icons/lucide/file-text'
import ILucideLayoutGrid from '~icons/lucide/layout-grid'
import ILucideNotebookPen from '~icons/lucide/notebook-pen'
import ILucideRss from '~icons/lucide/rss'
import ILucideMessageSquareText from '~icons/lucide/message-square-text'
import IMaterialSymbolsCallLogOutlineRounded from '~icons/material-symbols/call-log-outline-rounded'
import ILucideContactRound from '~icons/lucide/contact-round'
import IMaterialSymbolsScreenRecordRounded from '~icons/material-symbols/screen-record-rounded'
import ILucidePalette from '~icons/lucide/palette'

export interface Feature {
  id: string
  group: string
  defaultPath: string
  icon: Component
  titleKey: string
}

export const ALL_FEATURES: Feature[] = [
  { id: 'files', group: 'files', defaultPath: '/files/recent', icon: ILucideFolder, titleKey: 'page_title.files' },
  { id: 'audios', group: 'audios', defaultPath: '/audios', icon: ILucideMusic, titleKey: 'page_title.audios' },
  { id: 'images', group: 'images', defaultPath: '/images', icon: ILucideImage, titleKey: 'page_title.images' },
  { id: 'videos', group: 'videos', defaultPath: '/videos', icon: ILucideVideo, titleKey: 'page_title.videos' },
  { id: 'chat', group: 'chat', defaultPath: '/chat', icon: ILucideMessageCircle, titleKey: 'page_title.chat' },
  { id: 'docs', group: 'docs', defaultPath: '/docs', icon: ILucideFileText, titleKey: 'page_title.docs' },
  { id: 'apps', group: 'apps', defaultPath: '/apps', icon: ILucideLayoutGrid, titleKey: 'page_title.apps' },
  { id: 'notes', group: 'notes', defaultPath: '/notes', icon: ILucideNotebookPen, titleKey: 'page_title.notes' },
  { id: 'feeds', group: 'feeds', defaultPath: '/feeds', icon: ILucideRss, titleKey: 'page_title.feeds' },
  { id: 'messages', group: 'messages', defaultPath: '/messages', icon: ILucideMessageSquareText, titleKey: 'page_title.messages' },
  { id: 'calls', group: 'calls', defaultPath: '/calls', icon: IMaterialSymbolsCallLogOutlineRounded, titleKey: 'page_title.calls' },
  { id: 'contacts', group: 'contacts', defaultPath: '/contacts', icon: ILucideContactRound, titleKey: 'page_title.contacts' },
  { id: 'screen_mirror', group: 'screen_mirror', defaultPath: '/screen-mirror', icon: IMaterialSymbolsScreenRecordRounded, titleKey: 'page_title.screen_mirror' },
  { id: 'image_editor', group: 'image_editor', defaultPath: '/image-editor', icon: ILucidePalette, titleKey: 'page_title.image_editor' },
]

export const DEFAULT_RAIL_FEATURES = ['files', 'audios', 'images', 'videos', 'chat']

/** Features a NAS exposes: media, files and chat. */
export const NAS_FEATURE_IDS = new Set(['files', 'audios', 'images', 'videos', 'chat', 'docs'])

/** Features hidden on the Google Play channel (store policy). */
export const GOOGLE_EXCLUDED_FEATURE_IDS = new Set(['apps', 'messages', 'calls'])

/** Features hidden unless debug is enabled. */
export const DEBUG_EXCLUDED_FEATURE_IDS = new Set(['image_editor'])

export function getAvailableFeatures(deviceType?: DeviceType, channel?: AppChannelType, debug?: boolean): Feature[] {
  return ALL_FEATURES
    .filter((f) => !(channel === AppChannelType.GOOGLE && GOOGLE_EXCLUDED_FEATURE_IDS.has(f.id)))
    .filter((f) => (debug ?? false) || !DEBUG_EXCLUDED_FEATURE_IDS.has(f.id))
    .filter((f) => deviceType !== DeviceType.NAS || NAS_FEATURE_IDS.has(f.id))
}
