import { computed, ref, watch, type Ref } from 'vue'
import type { IFile } from '@/lib/file'

export function useSystemFiles<T extends Pick<IFile, 'isSystem'>>(items: Ref<T[]>, directory: () => string, enabled: () => boolean) {
  const expanded = ref(false)
  const groups = computed(() => {
    if (!enabled()) return { regular: items.value, system: [] as T[] }
    const regular: T[] = []
    const system: T[] = []
    for (const item of items.value) (item.isSystem ? system : regular).push(item)
    return { regular, system }
  })
  const visibleItems = computed(() => expanded.value ? [...groups.value.system, ...groups.value.regular] : groups.value.regular)
  const systemItems = computed(() => groups.value.system)
  watch(directory, () => { expanded.value = false })
  return { expanded, visibleItems, systemItems }
}
