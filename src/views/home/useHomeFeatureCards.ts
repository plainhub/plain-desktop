import { computed, type Component, type ComputedRef } from 'vue'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import {
  DEFAULT_HOME_FEATURES,
  getAvailableHomeFeatures,
  normalizeHomeFeatures,
  type HomeSectionFeature,
} from './features'

export interface HomeFeatureCard {
  id: string
  icon: Component
  titleKey: string
  sectionType: 'feature' | 'call_phone'
  to: string
  count?: number
  showStorageInfo: boolean
}

export function useHomeFeatureCards(filesPath: ComputedRef<string>) {
  const mainStore = useMainStore()
  const { app, counter } = storeToRefs(useTempStore())

  const availableFeatures = computed(() => getAvailableHomeFeatures(app.value?.deviceType, app.value?.channel, app.value?.debug))

  const homeFeatureCards = computed<HomeFeatureCard[]>(() => {
    const availableMap = new Map(availableFeatures.value.map((feature) => [feature.id, feature]))
    const sourceFeatures = Array.isArray(mainStore.homeFeatures) ? mainStore.homeFeatures : DEFAULT_HOME_FEATURES
    const selectedIds = normalizeHomeFeatures(sourceFeatures, Array.from(availableMap.keys()))
    const selectedFeatures = selectedIds.reduce<HomeSectionFeature[]>((acc, id) => {
      const feature = availableMap.get(id)
      if (feature) acc.push(feature)
      return acc
    }, [])

    return selectedFeatures.map((feature) =>
      feature.sectionType === 'feature'
        ? {
            ...feature,
            to: feature.id === 'files' ? filesPath.value : feature.defaultPath,
            count: feature.countKey ? counter.value[feature.countKey] : undefined,
            showStorageInfo: feature.id === 'files',
          }
        : {
            ...feature,
            to: '',
            count: undefined,
            showStorageInfo: false,
          }
    )
  })

  return { homeFeatureCards }
}
