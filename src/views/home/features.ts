import type { Component } from 'vue'
import ILucidePhoneCall from '~icons/lucide/phone-call'
import { ALL_FEATURES, DEBUG_EXCLUDED_FEATURE_IDS, GOOGLE_EXCLUDED_FEATURE_IDS, NAS_FEATURE_IDS, type Feature } from '@/views/app-rail/features'
import { AppChannelType, DeviceType } from '@/lib/status'
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

/** Home card order and set for a NAS: media, docs and files, no phone panels. */
export const NAS_HOME_FEATURES = ['audios', 'images', 'videos', 'docs', 'files']

export function getAvailableHomeFeatures(deviceType?: DeviceType, channel?: AppChannelType, debug?: boolean): HomeSectionFeature[] {
  const isNas = deviceType === DeviceType.NAS
  const routeFeatures = ALL_FEATURES
    .filter((feature) => HOME_FEATURE_IDS.has(feature.id))
    .filter((feature) => !isLocalMode() || isLocalFeatureId(feature.id))
    .filter((feature) => !(channel === AppChannelType.GOOGLE && GOOGLE_EXCLUDED_FEATURE_IDS.has(feature.id)))
    .filter((feature) => (debug ?? false) || !DEBUG_EXCLUDED_FEATURE_IDS.has(feature.id))
    .filter((feature) => !isNas || NAS_FEATURE_IDS.has(feature.id))
    .map((feature) => ({
      ...feature,
      sectionType: 'feature' as const,
      countKey: HOME_FEATURE_COUNT_KEYS[feature.id],
    }))

  const featureMap = new Map<string, HomeSectionFeature>([
    ...routeFeatures.map((feature) => [feature.id, feature] as const),
    ...(isNas || isLocalMode() ? [] : HOME_PANEL_FEATURES.map((feature) => [feature.id, feature] as const)),
  ])

  const defaults = isNas ? NAS_HOME_FEATURES : DEFAULT_HOME_FEATURES
  return defaults
    .map((id) => featureMap.get(id))
    .filter((feature): feature is HomeSectionFeature => !!feature)
}
