import { AppFeatureType } from '@/lib/app-feature.generated'
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
  id: AppFeatureType
  group: string
  defaultPath: string
  icon: Component
  titleKey: string
  /** Server-declared capability required to show this feature. */
  capability?: Capability
}

export const ALL_FEATURES: Feature[] = [
  { id: AppFeatureType.FILES, group: 'files', defaultPath: '/files/recent', icon: ILucideFolder, titleKey: 'page_title.files' },
  { id: AppFeatureType.AUDIO, group: 'audios', defaultPath: '/audios', icon: ILucideMusic, titleKey: 'page_title.audios' },
  { id: AppFeatureType.IMAGES, group: 'images', defaultPath: '/images', icon: ILucideImage, titleKey: 'page_title.images' },
  { id: AppFeatureType.VIDEOS, group: 'videos', defaultPath: '/videos', icon: ILucideVideo, titleKey: 'page_title.videos' },
  { id: AppFeatureType.CHAT, group: 'chat', defaultPath: '/chat', icon: ILucideMessageCircle, titleKey: 'page_title.chat' },
  { id: AppFeatureType.DOCS, group: 'docs', defaultPath: '/docs', icon: ILucideFileText, titleKey: 'page_title.docs' },
  { id: AppFeatureType.APPS, group: 'apps', defaultPath: '/apps', icon: ILucideLayoutGrid, titleKey: 'page_title.apps', capability: Capability.PACKAGES },
  { id: AppFeatureType.NOTES, group: 'notes', defaultPath: '/notes', icon: ILucideNotebookPen, titleKey: 'page_title.notes' },
  { id: AppFeatureType.FEEDS, group: 'feeds', defaultPath: '/feeds', icon: ILucideRss, titleKey: 'page_title.feeds' },
  { id: AppFeatureType.SMS, group: 'messages', defaultPath: '/messages', icon: ILucideMessageSquareText, titleKey: 'page_title.messages', capability: Capability.TELEPHONY },
  { id: AppFeatureType.CALLS, group: 'calls', defaultPath: '/calls', icon: IMaterialSymbolsCallLogOutlineRounded, titleKey: 'page_title.calls', capability: Capability.TELEPHONY },
  { id: AppFeatureType.CONTACTS, group: 'contacts', defaultPath: '/contacts', icon: ILucideContactRound, titleKey: 'page_title.contacts', capability: Capability.TELEPHONY },
  { id: AppFeatureType.SCREEN_MIRROR, group: 'screen_mirror', defaultPath: '/screen-mirror', icon: IMaterialSymbolsScreenRecordRounded, titleKey: 'page_title.screen_mirror', capability: Capability.SCREEN_MIRROR },
  { id: AppFeatureType.IMAGE_EDITOR, group: 'image_editor', defaultPath: '/image-editor', icon: ILucidePalette, titleKey: 'page_title.image_editor' },
]

export const DEFAULT_RAIL_FEATURES = [AppFeatureType.FILES, AppFeatureType.AUDIO, AppFeatureType.IMAGES, AppFeatureType.VIDEOS, AppFeatureType.CHAT]

export function getAvailableFeatures(features?: string[]): Feature[] {
  return ALL_FEATURES.filter((f) => !f.capability || hasFeature(f.capability, features))
}
