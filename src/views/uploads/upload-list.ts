import { useTempStore } from '@/stores/temp'
import { computed, ref, watch } from 'vue'
import { useMainStore } from '@/stores/main'
import { useI18n } from 'vue-i18n'
import { addUploadTask } from '@/lib/upload/upload-queue'
import { sortByName } from '@/lib/array'
import { batchCreatedAt, keyOf, partitionBatches, type TaskListItem } from '@/lib/upload/batch'

export function useUploadList() {
  const tempStore = useTempStore()
  const store = useMainStore()
  const { t } = useI18n()
  const filterType = ref('in_progress')
  const types = ['in_progress', 'completed']
  const listItemsRef = ref()

  function chooseFilterType(value: string) {
    filterType.value = value
    if (listItemsRef.value) listItemsRef.value.scrollTop = 0
  }

  const tasks = computed(() => partitionBatches(tempStore.uploads))
  const visibleTasks = computed<TaskListItem[]>(() => (filterType.value === 'in_progress' ? tasks.value.inProgress : tasks.value.completed))
  const completedCount = computed(() => tasks.value.completed.length)
  const totalCount = computed(() => tasks.value.inProgress.length + tasks.value.completed.length)

  function getLabel(type: string) {
    return t(type) + (type === 'completed' ? ` (${completedCount.value})` : ` (${totalCount.value - completedCount.value})`)
  }

  watch(
    () => tempStore.uploads,
    (newUploads) => {
      const created = newUploads.filter((item) => item.status === 'created')
      if (created.length === 0) return
      const batches = new Map<string, typeof newUploads>()
      for (const it of created) {
        const k = keyOf(it)
        const list = batches.get(k)
        if (list) list.push(it)
        else batches.set(k, [it])
      }
      const ordered = sortByName(Array.from(batches.entries()), (e) => batchCreatedAt(e[1]))
      for (const [_, newItems] of ordered) {
        for (const item of newItems) {
          if (item.status !== 'created') continue
          addUploadTask(item, true)
          item.status = 'pending'
        }
      }
    },
  )

  return { store, filterType, types, listItemsRef, visibleTasks, chooseFilterType, getLabel }
}
