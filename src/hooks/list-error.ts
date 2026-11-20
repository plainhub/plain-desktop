import { ref, type Ref } from 'vue'
import { AccessErrorCode } from '@/lib/api/access-error-code'

export function useListError<T>(items: Ref<T[]>, total?: Ref<number>) {
  const listError = ref('')
  function setListError(error: string) {
    if (error === AccessErrorCode.NoPermission) {
      items.value = []
      if (total) total.value = 0
    }
    listError.value = items.value.length ? '' : error
  }
  return { listError, setListError }
}
