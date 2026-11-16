import { execFileSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { expect, it, vi } from 'vitest'
import { computed, ref } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { AppFeatureType } from '@/lib/app-feature.generated'
import { useMainStore } from '@/stores/main'
import { useHomeFeatureCards } from '@/views/home/useHomeFeatureCards'
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({ capabilities: ['TELEPHONY', 'PACKAGES', 'SCREEN_MIRROR'] }), counter: ref({ audios: 3, messages: 2 }) }) }))
import { DEFAULT_RAIL_FEATURES, ALL_FEATURES } from '@/views/app-rail/features'
import { DEFAULT_HOME_FEATURES } from '@/views/home/feature-list'

it('uses common uppercase feature names independently of route groups', () => {
  expect(DEFAULT_RAIL_FEATURES).toEqual(['FILES', 'AUDIO', 'IMAGES', 'VIDEOS', 'CHAT'])
  expect(DEFAULT_HOME_FEATURES).toEqual(['AUDIO', 'IMAGES', 'VIDEOS', 'DOCS', 'FILES', 'APPS', 'NOTES', 'FEEDS', 'SMS', 'CALLS', 'CONTACTS', 'SCREEN_MIRROR', 'IMAGE_EDITOR', 'CALL_PHONE'])
  expect(ALL_FEATURES.find(feature => feature.group === 'audios')?.id).toBe('AUDIO')
  expect(ALL_FEATURES.find(feature => feature.group === 'messages')?.id).toBe('SMS')
})

it('renders stored feature order with existing routes, counts and file navigation', () => {
  setActivePinia(createPinia())
  const store = useMainStore()
  expect(store.railFeatures).toEqual(['FILES', 'AUDIO', 'IMAGES', 'VIDEOS', 'CHAT'])
  store.homeFeatures = [AppFeatureType.SMS, AppFeatureType.AUDIO, AppFeatureType.FILES, AppFeatureType.CALL_PHONE]
  const { homeFeatureCards } = useHomeFeatureCards(computed(() => '/files?q=test'))
  expect(homeFeatureCards.value.map(({ id, to, count, showStorageInfo }) => ({ id, to, count, showStorageInfo }))).toEqual([
    { id: 'SMS', to: '/messages', count: 2, showStorageInfo: false },
    { id: 'AUDIO', to: '/audios', count: 3, showStorageInfo: false },
    { id: 'FILES', to: '/files?q=test', count: undefined, showStorageInfo: true },
    { id: 'CALL_PHONE', to: '', count: undefined, showStorageInfo: false },
  ])
})

it('keeps the generated TypeScript enum equal to the Rust definition', () => {
  execFileSync(process.execPath, [
    fileURLToPath(new URL('../../../plain-core/scripts/gen-app-features.mjs', import.meta.url)),
    '--target=desktop',
  ])
})
