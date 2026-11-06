<template>
  <div class="scroll-content">
    <div class="headline-row">
      <h2>{{ $t('customize_ui') }}</h2>
      <div class="button-group tab-group" role="tablist">
        <button type="button" :class="{ selected: activeTab === 'sidebar' }" @click="activeTab = 'sidebar'">
          {{ $t('customize_ui_tab_sidebar') }}
        </button>
        <button type="button" :class="{ selected: activeTab === 'home' }" @click="activeTab = 'home'">
          {{ $t('customize_ui_tab_home') }}
        </button>
      </div>
    </div>
    <FeatureSortToggleList
      :key="activeTab"
      v-model="activeEnabledIds"
      :features="activeFeatures"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { getAvailableFeatures } from '@/views/app-rail/features'
import { getAvailableHomeFeatures, normalizeHomeFeatures } from '@/views/home/features'
import FeatureSortToggleList from '@/views/app-rail/FeatureSortToggleList.vue'

type UITab = 'sidebar' | 'home'

const store = useMainStore()
const { app } = storeToRefs(useTempStore())
const activeTab = ref<UITab>('sidebar')

const sidebarFeatures = computed(() => getAvailableFeatures(app.value?.capabilities, app.value?.debug))
const homeFeatures = computed(() => getAvailableHomeFeatures(app.value?.capabilities))

const activeFeatures = computed(() =>
  activeTab.value === 'sidebar' ? sidebarFeatures.value : homeFeatures.value
)

const activeEnabledIds = computed({
  get: () => activeTab.value === 'sidebar'
    ? store.railFeatures.filter((id) => sidebarFeatures.value.some((feature) => feature.id === id))
    : normalizeHomeFeatures(store.homeFeatures, homeFeatures.value.map((feature) => feature.id)),
  set: (ids: string[]) => {
    if (activeTab.value === 'sidebar') {
      store.railFeatures = [...ids]
    } else {
      store.homeFeatures = [...ids]
    }
  },
})
</script>

<style lang="scss" scoped>
.scroll-content {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
}

.headline-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 16px;
}

h2 {
  margin: 0;
  font-size: 1.25rem;
}

.tab-group {
  margin-left: auto;
  background: var(--md-sys-color-surface-container);
}
</style>
