import { AppFeatureType } from '@/lib/app-feature.generated'
import type { Component } from 'vue'
import ILucidePhoneCall from '~icons/lucide/phone-call'
import { getAvailableFeatures, type Feature } from '@/views/app-rail/features'
import { Capability } from '@/lib/data'
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
  id: AppFeatureType
  icon: Component
  titleKey: string
  sectionType: 'call_phone'
}

export type HomeSectionFeature = HomeFeature | HomePanelFeature

const HOME_FEATURE_IDS = new Set<AppFeatureType>(DEFAULT_HOME_FEATURES.filter((id) => id !== AppFeatureType.CALL_PHONE))

const HOME_FEATURE_COUNT_KEYS: Partial<Record<AppFeatureType, HomeFeatureCountKey>> = {
  [AppFeatureType.AUDIO]: 'audios',
  [AppFeatureType.IMAGES]: 'images',
  [AppFeatureType.VIDEOS]: 'videos',
  [AppFeatureType.DOCS]: 'docs',
  [AppFeatureType.APPS]: 'packages',
  [AppFeatureType.NOTES]: 'notes',
  [AppFeatureType.FEEDS]: 'feedEntries',
  [AppFeatureType.SMS]: 'messages',
  [AppFeatureType.CALLS]: 'calls',
  [AppFeatureType.CONTACTS]: 'contacts',
}

const HOME_PANEL_FEATURES: HomePanelFeature[] = [
  { id: AppFeatureType.CALL_PHONE, icon: ILucidePhoneCall, titleKey: 'call_phone', sectionType: 'call_phone' },
]

export function getAvailableHomeFeatures(features?: string[]): HomeSectionFeature[] {
  const routeFeatures = getAvailableFeatures(features)
    .filter((feature) => HOME_FEATURE_IDS.has(feature.id))
    .map((feature) => ({
      ...feature,
      sectionType: 'feature' as const,
      countKey: HOME_FEATURE_COUNT_KEYS[feature.id],
    }))

  const hasCallPhone = !!features?.includes(Capability.TELEPHONY)
  const featureMap = new Map<AppFeatureType, HomeSectionFeature>([
    ...routeFeatures.map((feature) => [feature.id, feature] as const),
    ...(hasCallPhone ? HOME_PANEL_FEATURES.map((feature) => [feature.id, feature] as const) : []),
  ])

  // Unsupported ids simply drop out of the map.
  return DEFAULT_HOME_FEATURES
    .map((id) => featureMap.get(id))
    .filter((feature): feature is HomeSectionFeature => !!feature)
}
