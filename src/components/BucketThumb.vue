<template>
  <span class="thumb" :class="`count-${thumbs.length}`">
    <template v-if="thumbs.length">
      <img v-for="(p, i) in thumbs" :key="i" class="thumb-img" :src="thumbUrl(p)" loading="lazy" alt="" onerror="this.style.display='none'" />
    </template>
    <i-material-symbols:folder-rounded v-else />
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { getFileId, getFileUrl } from '@/lib/api/file'

const props = defineProps({
  items: {
    type: Array as () => string[],
    default: () => [],
  },
})

const tempStore = useTempStore()
const { urlTokenKey } = storeToRefs(tempStore)

const thumbs = computed(() => props.items?.slice(0, 4) ?? [])

function thumbUrl(path: string): string {
  return getFileUrl(getFileId(urlTokenKey.value, path), '&w=128&h=128')
}
</script>

<style lang="scss" scoped>
.thumb {
  flex-shrink: 0;
  width: 40px;
  height: 40px;
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  grid-template-rows: repeat(2, minmax(0, 1fr));
  gap: 2px;
  border-radius: 10px;
  overflow: hidden;
  background: color-mix(in srgb, var(--md-sys-color-primary) 12%, transparent);

  &.count-1 {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
  }

  &.count-2 {
    grid-template-rows: minmax(0, 1fr);
  }

  &.count-3 .thumb-img:first-child {
    grid-column: 1 / -1;
  }

  &.count-0 {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 22px;
    color: var(--md-sys-color-on-surface-variant);
  }

  .thumb-img {
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    display: block;
    object-fit: cover;
    object-position: center;
  }
}
</style>
