<template>
  <LightboxFileInfoItem v-if="current?.type && !isTrashed" :label="$t('tags')">
    <template #label>
      {{ $t('tags') }}
      <TagRelationsDropdown
        :type="current.type"
        :tags="tagsForType"
        :item="{ key: current.data?.id ?? '' }"
        :selected="selectedTags"
      />
    </template>
    <ItemTags :tags="selectedTags" />
  </LightboxFileInfoItem>
</template>

<script setup lang="ts">
import { isTrashedMediaPath } from '@/lib/media/trash-path'
import { computed } from 'vue'
import ItemTags from '@/components/ItemTags.vue'
import type { ISource } from './types'
import type { ITag } from '@/lib/interfaces'

const props = defineProps({
  current: {
    type: Object as () => ISource | undefined,
    required: true,
  },
  itemTags: {
    type: Array as () => { tagId: string; key: string }[],
    default: () => [],
  },
  tagsMap: {
    type: Object as () => Map<string, ITag[]>,
    required: true,
  },
})

const isTrashed = computed(() => {
  return isTrashedMediaPath(props.current?.path)
})

const tagsForType = computed(() => {
  return props.tagsMap.get(props.current?.type ?? '') ?? []
})

const selectedTags = computed(() => {
  const ids = new Set(props.itemTags.map((r) => r.tagId))
  return tagsForType.value.filter((t) => ids.has(t.id))
})
</script>

<style lang="scss" scoped>
.info-tag-btn {
  margin-left: 4px;
}
</style> 