import type { Component } from 'vue'
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
  requireDebug?: boolean
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
  { id: 'image_editor', group: 'image_editor', defaultPath: '/image-editor', icon: ILucidePalette, titleKey: 'page_title.image_editor', requireDebug: true },
]

export const DEFAULT_RAIL_FEATURES = ['files', 'audios', 'images', 'videos', 'chat']

/** Features a NAS exposes: media + files. Chat is NOT filtered by this set —
 *  it is force-retained on the rail via `withNasChatRetention`. */
export const NAS_FEATURE_IDS = new Set(['files', 'audios', 'images', 'videos', 'docs'])

export function getAvailableFeatures(isNas: boolean, debug: boolean = false): Feature[] {
  const features = ALL_FEATURES.filter((f) => !(f.requireDebug && !debug))
  if (!isNas) return features
  return features.filter((f) => NAS_FEATURE_IDS.has(f.id))
}

/** A NAS always keeps chat on the rail, regardless of saved customization
 *  or the NAS feature whitelist — apply after availability filtering. */
export function withNasChatRetention(features: Feature[], isNas: boolean): Feature[] {
  if (!isNas || features.some((f) => f.id === 'chat')) return features
  const chat = ALL_FEATURES.find((f) => f.id === 'chat')
  return chat ? [...features, chat] : features
}
