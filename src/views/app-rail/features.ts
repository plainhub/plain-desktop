import type { Component } from 'vue'
import { Capability } from '@/lib/data'
import { hasFeature } from '@/lib/feature'
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
  /** Server-declared capability required to show this feature. */
  capability?: Capability
}

export const ALL_FEATURES: Feature[] = [
  { id: 'files', group: 'files', defaultPath: '/files/recent', icon: ILucideFolder, titleKey: 'page_title.files' },
  { id: 'audios', group: 'audios', defaultPath: '/audios', icon: ILucideMusic, titleKey: 'page_title.audios' },
  { id: 'images', group: 'images', defaultPath: '/images', icon: ILucideImage, titleKey: 'page_title.images' },
  { id: 'videos', group: 'videos', defaultPath: '/videos', icon: ILucideVideo, titleKey: 'page_title.videos' },
  { id: 'chat', group: 'chat', defaultPath: '/chat', icon: ILucideMessageCircle, titleKey: 'page_title.chat' },
  { id: 'docs', group: 'docs', defaultPath: '/docs', icon: ILucideFileText, titleKey: 'page_title.docs' },
  { id: 'apps', group: 'apps', defaultPath: '/apps', icon: ILucideLayoutGrid, titleKey: 'page_title.apps', capability: Capability.PACKAGES },
  { id: 'notes', group: 'notes', defaultPath: '/notes', icon: ILucideNotebookPen, titleKey: 'page_title.notes', capability: Capability.NOTES },
  { id: 'feeds', group: 'feeds', defaultPath: '/feeds', icon: ILucideRss, titleKey: 'page_title.feeds', capability: Capability.FEEDS },
  { id: 'messages', group: 'messages', defaultPath: '/messages', icon: ILucideMessageSquareText, titleKey: 'page_title.messages', capability: Capability.SMS },
  { id: 'calls', group: 'calls', defaultPath: '/calls', icon: IMaterialSymbolsCallLogOutlineRounded, titleKey: 'page_title.calls', capability: Capability.CALLS },
  { id: 'contacts', group: 'contacts', defaultPath: '/contacts', icon: ILucideContactRound, titleKey: 'page_title.contacts', capability: Capability.CONTACTS },
  { id: 'screen_mirror', group: 'screen_mirror', defaultPath: '/screen-mirror', icon: IMaterialSymbolsScreenRecordRounded, titleKey: 'page_title.screen_mirror', capability: Capability.SCREEN_MIRROR },
  { id: 'image_editor', group: 'image_editor', defaultPath: '/image-editor', icon: ILucidePalette, titleKey: 'page_title.image_editor', capability: Capability.IMAGE_EDITOR },
]

export const DEFAULT_RAIL_FEATURES = ['files', 'audios', 'images', 'videos', 'chat']

export function getAvailableFeatures(features?: string[]): Feature[] {
  return ALL_FEATURES
    .filter((f) => !f.capability || hasFeature(f.capability, features))
}
