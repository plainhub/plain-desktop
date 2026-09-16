<template>
  <div class="grids">
    <template v-for="item in homeFeatureCards" :key="item.id">
      <FeatureCard v-if="item.sectionType === 'feature'" :to="item.to" :title="$t(item.titleKey)" :count="item.count">
        <template #icon>
          <component :is="item.icon" />
        </template>
        <div v-if="item.showStorageInfo && counter.total >= 0" class="storage-info">
          {{ $t('storage_free_total', { free: formatFileSize(counter.free), total: formatFileSize(counter.total) }) }}
        </div>
      </FeatureCard>

      <CallPhoneCard v-else />
    </template>
  </div>
</template>

<script setup lang="ts">
import { formatFileSize } from '@/lib/format'
import { computed } from 'vue'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { buildQuery } from '@/lib/search'
import { encodeBase64 } from '@/lib/strutil'
import { DriveType } from '@/lib/status'
import { useHomeData } from './home'
import { useHomeFeatureCards } from './useHomeFeatureCards'
import CallPhoneCard from './CallPhoneCard.vue'
import FeatureCard from './FeatureCard.vue'

const { app, counter } = storeToRefs(useTempStore())

const { mounts } = useHomeData()

const filesPath = computed(() => {
  const internalRoot = mounts.value.find((m) => m.driveType === DriveType.INTERNAL_STORAGE)?.mountPoint || app.value.internalStoragePath
  const q = buildQuery([
    { name: 'parent', op: '', value: internalRoot },
    { name: 'type', op: '', value: 'INTERNAL_STORAGE' },
    { name: 'root_path', op: '', value: internalRoot },
  ])
  return `/files?q=${encodeBase64(q)}`
})

const { homeFeatureCards } = useHomeFeatureCards(filesPath)
</script>

<style lang="scss" scoped src="./HomeView.scss"></style>
