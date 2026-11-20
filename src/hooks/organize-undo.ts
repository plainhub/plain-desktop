import { showActionError } from '@/hooks/show-action-error'
import { ref, computed } from 'vue'
import { initMutation } from '@/lib/api/mutation'
import { moveMediaItemsGQL } from '@/lib/api/mutation'
import { restoreMediaItemsGQL } from '@/lib/api/mutation'
import emitter from '@/plugins/eventbus'
import type { DataType } from '@/lib/data'

export interface OrganizeAction {
  /** 'trash' actions are undone via restoreMediaItems, 'move' via moveMediaItems back to origDir */
  kind: 'trash' | 'move'
  type: DataType
  query: string
  /** only for kind === 'move': directory the item was moved from */
  origDir?: string
}

/**
 * Session-scoped undo stack for organizing media in the lightbox.
 * Actions take effect immediately and can be undone one at a time
 * (Ctrl/⌘+Z or the undo button) until the lightbox closes.
 */
const stack = ref<OrganizeAction[]>([])

export function useOrganizeUndo() {
  const count = computed(() => stack.value.length)

  function record(action: OrganizeAction) {
    // A blank query would target everything once replayed — never stack it.
    if (!action.query.trim()) return
    stack.value.push(action)
  }

  function removeRecorded(action: OrganizeAction) {
    const idx = stack.value.lastIndexOf(action)
    if (idx !== -1) stack.value.splice(idx, 1)
  }

  function applyUndo(action: OrganizeAction, mutate: (params: Record<string, unknown>) => void) {
    if (action.kind === 'trash') {
      mutate({ type: action.type, query: action.query })
    } else if (action.origDir) {
      mutate({ type: action.type, query: action.query, destDir: action.origDir })
    }
  }

  /** Undo the most recent action. Returns it, or null when the stack is empty. */
  function undoLast(): OrganizeAction | null {
    const action = stack.value.pop()
    if (!action) return null
    if (action.kind === 'trash') {
      const { mutate } = initMutation({ document: restoreMediaItemsGQL }, showActionError)
      mutate({ type: action.type, query: action.query })
    } else if (action.origDir) {
      const { mutate } = initMutation({ document: moveMediaItemsGQL }, showActionError)
      mutate({ type: action.type, query: action.query, destDir: action.origDir })
    }
    emitter.emit('media_items_actioned', { type: action.type, action: 'undo', query: action.query })
    return action
  }

  function clear() {
    stack.value = []
  }

  return { count, record, removeRecorded, undoLast, clear }
}
