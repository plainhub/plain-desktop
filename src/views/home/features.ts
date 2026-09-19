import type { Component } from 'vue'
import ILucidePhoneCall from '~icons/lucide/phone-call'
import { ALL_FEATURES, DEBUG_EXCLUDED_FEATURE_IDS, GOOGLE_EXCLUDED_FEATURE_IDS, type Feature } from '@/views/app-rail/features'
import { AppChannelType } from '@/lib/status'
import { DeviceFeature } from '@/lib/data'
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
  sectionType: 'call_phone'
}

export type HomeSectionFeature = HomeFeature | HomePanelFeature

const HOME_FEATURE_IDS = new Set(DEFAULT_HOME_FEATURES.filter((id) => id !== 'call_phone'))

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
  { id: 'call_phone', icon: ILucidePhoneCall, titleKey: 'call_phone', sectionType: 'call_phone' },
]

/** Availability mirrors the rail: capability-gated features need the
 *  server to declare them in `app.features`. */
export function getAvailableHomeFeatures(features?: string[], channel?: AppChannelType, debug?: boolean): HomeSectionFeature[] {
  const routeFeatures = ALL_FEATURES
    .filter((feature) => HOME_FEATURE_IDS.has(feature.id))
    .filter((feature) => !isLocalMode() || isLocalFeatureId(feature.id))
    .filter((feature) => !(channel === AppChannelType.GOOGLE && GOOGLE_EXCLUDED_FEATURE_IDS.has(feature.id)))
    .filter((feature) => (debug ?? false) || !DEBUG_EXCLUDED_FEATURE_IDS.has(feature.id))
    .filter((feature) => !feature.capability || !!features?.includes(feature.capability))
    .map((feature) => ({
      ...feature,
      sectionType: 'feature' as const,
      countKey: HOME_FEATURE_COUNT_KEYS[feature.id],
    }))

  const hasCallPhone = !!features?.includes(DeviceFeature.CALL_PHONE)
  const featureMap = new Map<string, HomeSectionFeature>([
    ...routeFeatures.map((feature) => [feature.id, feature] as const),
    ...(hasCallPhone && !isLocalMode() ? HOME_PANEL_FEATURES.map((feature) => [feature.id, feature] as const) : []),
  ])

  // Unsupported ids simply drop out of the map.
  return DEFAULT_HOME_FEATURES
    .map((id) => featureMap.get(id))
    .filter((feature): feature is HomeSectionFeature => !!feature)
}
