<template>
  <v-modal @close="close">
    <template #headline>
      <div class="headline-row">
        <span>{{ $t('customize_ui') }}</span>
        <div class="button-group tab-group" role="tablist">
          <button type="button" :class="{ selected: activeTab === 'sidebar' }" @click="activeTab = 'sidebar'">
            {{ $t('customize_ui_tab_sidebar') }}
          </button>
          <button type="button" :class="{ selected: activeTab === 'home' }" @click="activeTab = 'home'">
            {{ $t('customize_ui_tab_home') }}
          </button>
        </div>
      </div>
    </template>
    <template #content>
      <FeatureSortToggleList
        :key="activeTab"
        v-model="activeEnabledIds"
        :features="activeFeatures"
      />
    </template>
    <template #actions>
      <v-filled-button @click="close">{{ $t('close') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { popModal } from '@/components/modal'
import { useMainStore } from '@/stores/main'
import { useTempStore } from '@/stores/temp'
import { getAvailableFeatures } from './features'
import { DEFAULT_HOME_FEATURES, getAvailableHomeFeatures, normalizeHomeFeatures } from '../home/features'
import FeatureSortToggleList from './FeatureSortToggleList.vue'

type UITab = 'sidebar' | 'home'

const store = useMainStore()
const { app } = storeToRefs(useTempStore())
const activeTab = ref<UITab>('sidebar')

const sidebarFeatures = computed(() => getAvailableFeatures(app.value?.capabilities, app.value?.channel, app.value?.debug))
const homeFeatures = computed(() => getAvailableHomeFeatures(app.value?.capabilities, app.value?.channel, app.value?.debug))

const sidebarEnabledIds = ref(
  store.railFeatures.filter((id: string) => sidebarFeatures.value.some((feature) => feature.id === id))
)
const sourceHomeFeatures = Array.isArray(store.homeFeatures) ? store.homeFeatures : DEFAULT_HOME_FEATURES
const homeEnabledIds = ref(
  normalizeHomeFeatures(sourceHomeFeatures, homeFeatures.value.map((feature) => feature.id))
)

const activeFeatures = computed(() =>
  activeTab.value === 'sidebar' ? sidebarFeatures.value : homeFeatures.value
)

const activeEnabledIds = computed({
  get: () => (activeTab.value === 'sidebar' ? sidebarEnabledIds.value : homeEnabledIds.value),
  set: (ids: string[]) => {
    if (activeTab.value === 'sidebar') {
      sidebarEnabledIds.value = ids
    } else {
      homeEnabledIds.value = ids
    }
  },
})

watch(sidebarEnabledIds, (ids) => { store.railFeatures = [...ids] })
watch(homeEnabledIds, (ids) => { store.homeFeatures = [...ids] })

function close() {
  popModal()
}
</script>

<style lang="scss" scoped>
.headline-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.tab-group {
  margin-left: auto;
  background: var(--md-sys-color-surface-container);
}
</style>
