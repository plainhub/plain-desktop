import type { Component } from 'vue'
import ILucideClipboard from '~icons/lucide/clipboard'
import ILucidePhoneCall from '~icons/lucide/phone-call'
import { ALL_FEATURES, type Feature } from '@/views/app-rail/features'
import { AppChannelType } from '@/lib/status'
import { isLocalFeatureId, isLocalMode } from '@/lib/device/local-mode'
import { DEFAULT_HOME_FEATURES, normalizeHomeFeatures } from './feature-list'

export { DEFAULT_HOME_FEATURES, normalizeHomeFeatures }

export type HomeFeatureCountKey =
  | 'audios'
  | 'images'
  | 'videos'
  | 'docs'
  | 'packages'
  | 'notes'
  | 'feedEntries'
  | 'messages'
  | 'calls'
  | 'contacts'

export interface HomeFeature extends Feature {
  sectionType: 'feature'
  countKey?: HomeFeatureCountKey
}

export interface HomePanelFeature {
  id: string
  icon: Component
  titleKey: string
  sectionType: 'clipboard' | 'call_phone'
}

export type HomeSectionFeature = HomeFeature | HomePanelFeature

const HOME_FEATURE_IDS = new Set(DEFAULT_HOME_FEATURES.filter((id) => id !== 'clipboard' && id !== 'call_phone'))

const HOME_FEATURE_COUNT_KEYS: Partial<Record<string, HomeFeatureCountKey>> = {
  audios: 'audios',
  images: 'images',
  videos: 'videos',
  docs: 'docs',
  apps: 'packages',
  notes: 'notes',
  feeds: 'feedEntries',
  messages: 'messages',
  calls: 'calls',
  contacts: 'contacts',
}

const HOME_PANEL_FEATURES: HomePanelFeature[] = [
  { id: 'clipboard', icon: ILucideClipboard, titleKey: 'send_to_phone_clipboard', sectionType: 'clipboard' },
  { id: 'call_phone', icon: ILucidePhoneCall, titleKey: 'call_phone', sectionType: 'call_phone' },
]

export function getAvailableHomeFeatures(channel: AppChannelType, debug: boolean = false): HomeSectionFeature[] {
  const routeFeatures = ALL_FEATURES
    .filter((feature) => HOME_FEATURE_IDS.has(feature.id))
    .filter((feature) => !isLocalMode() || isLocalFeatureId(feature.id))
    .filter((feature) => !(feature.requireNonGoogle && channel === AppChannelType.GOOGLE))
    .filter((feature) => !(feature.requireDebug && !debug))
    .map((feature) => ({
      ...feature,
      sectionType: 'feature' as const,
      countKey: HOME_FEATURE_COUNT_KEYS[feature.id],
    }))

  const featureMap = new Map<string, HomeSectionFeature>([
    ...routeFeatures.map((feature) => [feature.id, feature] as const),
    ...(isLocalMode() ? [] : HOME_PANEL_FEATURES.map((feature) => [feature.id, feature] as const)),
  ])

  return DEFAULT_HOME_FEATURES
    .map((id) => featureMap.get(id))
    .filter((feature): feature is HomeSectionFeature => !!feature)
}
