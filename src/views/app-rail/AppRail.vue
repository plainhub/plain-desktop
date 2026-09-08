<template>
  <nav class="app-rail">
    <router-link v-if="!localMode" to="/" class="rail-brand" aria-label="PlainApp home">
      <span class="brand-logo" aria-hidden="true"></span>
    </router-link>

    <div class="rail-items">
      <router-link
        v-for="feat in railFeatures"
        :key="feat.id"
        v-tooltip="$t(feat.titleKey)"
        :to="lastRoute(feat.defaultPath, feat.group)"
        class="rail-item"
        :class="{ active: isActive(feat) }"
        :aria-label="$t(feat.titleKey)"
      >
        <component :is="feat.icon" />
        <div class="rail-label">{{ $t(feat.titleKey) }}</div>
      </router-link>
    </div>

    <div class="rail-spacer"></div>

    <RailSettingsPopup />
  </nav>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount } from 'vue'
import { useRouter } from 'vue-router'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { AppChannelType } from '@/lib/status'
import { ALL_FEATURES, getAvailableFeatures, type Feature } from './features'
import { isLocalFeatureId, isLocalMode } from '@/lib/device/local-mode'
import RailSettingsPopup from './RailSettingsPopup.vue'
const isTauri = __IS_TAURI__
const localMode = isLocalMode()

const store = useMainStore()
const router = useRouter()
const tempStore = useTempStore()
const { app } = storeToRefs(tempStore)

const availableFeatures = computed(() => getAvailableFeatures(app.value?.channel ?? AppChannelType.GITHUB, app.value?.debug ?? false))

const railFeatures = computed<Feature[]>(() => {
  const ids = localMode ? store.railFeatures.filter(isLocalFeatureId) : store.railFeatures
  return ids
    .map((id) => ALL_FEATURES.find((f) => f.id === id))
    .filter((f): f is Feature => !!f && availableFeatures.value.some((a) => a.id === f.id))
})

function isActive(feat: Feature) {
  try {
    const path = router.currentRoute.value.path
    const prefix = '/' + feat.group.replace('_', '-')
    return path.startsWith(prefix)
  } catch {
    return false
  }
}

function lastRoute(defaultPath: string, group: string) {
  try {
    const saved = store.lastRoutes[group]
    if (!saved) return defaultPath
    const resolved = router.resolve(saved)
    const g = (resolved.meta?.group || '') as string
    return g === group ? saved : defaultPath
  } catch {
    return defaultPath
  }
}

const removeAfterEach = router.afterEach((to) => {
  const group = (to.meta?.group || '') as string
  if (group) {
    store.lastRoutes[group] = to.fullPath
  }
})

onBeforeUnmount(() => {
  removeAfterEach()
})
</script>

<style lang="scss" scoped>
.app-rail {
  grid-area: rail;
  width: 80px;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 8px 0;
}

.rail-brand {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 12px;
  width: 40px;
  height: 40px;
  margin: 4px 0 10px;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
}

.rail-brand:hover {
  background: color-mix(in srgb, var(--md-sys-color-primary) 10%, transparent);
}

.app-rail .brand-logo {
  width: 40px;
  height: 40px;
  display: block;
  background-image: url('/logo.svg');
  background-repeat: no-repeat;
  background-position: center;
  background-size: contain;
}

.rail-items {
  display: flex;
  flex-direction: column;
  align-items: center;
  overflow-y: auto;
  overflow-x: hidden;
  flex: 1;
  width: 100%;
  scrollbar-width: none;

  &::-webkit-scrollbar {
    display: none;
  }
}

.rail-item {
  width: 56px;
  height: 72px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  border-radius: 12px;
  color: var(--md-sys-color-on-surface-variant);
  margin: 6px 0;
  gap: 4px;
  text-decoration: none;
  flex-shrink: 0;
  padding-inline: 4px;
  word-break: break-all;

  svg {
    width: 22px;
    height: 22px;
  }
}

.rail-label {
  font-size: 12px;
  line-height: 1;
  text-align: center;
}

.rail-item:hover,
.rail-item.active {
  background: color-mix(in srgb, var(--md-sys-color-primary) 12%, transparent);
  color: var(--md-sys-color-primary);
}

.rail-spacer {
  flex-shrink: 0;
  height: 8px;
}

@media (max-width: 768px) {
  .app-rail {
    flex-direction: row;
    width: auto;
    min-width: 0;
    padding: 4px 8px calc(4px + env(safe-area-inset-bottom));
  }

  .rail-brand,
  .rail-spacer {
    display: none;
  }

  .rail-items {
    flex-direction: row;
    overflow-x: auto;
    overflow-y: hidden;
  }

  .rail-item {
    flex: 1 1 0;
    min-width: 56px;
    height: 56px;
    margin: 0;
  }
}
</style>
