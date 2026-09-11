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
 * Actions are recorded silently; the user can step back one at a time
 * (Ctrl/⌘+Z or the undo button) and gets a single summary confirmation
 * with "undo all" when finishing the session (closing the lightbox).
 */
const stack = ref<OrganizeAction[]>([])

export function useOrganizeUndo() {
  const count = computed(() => stack.value.length)

  function record(action: OrganizeAction) {
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
      const { mutate } = initMutation({ document: restoreMediaItemsGQL })
      mutate({ type: action.type, query: action.query })
    } else if (action.origDir) {
      const { mutate } = initMutation({ document: moveMediaItemsGQL })
      mutate({ type: action.type, query: action.query, destDir: action.origDir })
    }
    emitter.emit('media_items_actioned', { type: action.type, action: 'undo', query: action.query })
    return action
  }

  /** Undo every recorded action of this session in reverse order. */
  function undoAll() {
    const restore = initMutation({ document: restoreMediaItemsGQL })
    const move = initMutation({ document: moveMediaItemsGQL })
    while (stack.value.length) {
      const action = stack.value.pop()!
      if (action.kind === 'trash') {
        restore.mutate({ type: action.type, query: action.query })
      } else if (action.origDir) {
        move.mutate({ type: action.type, query: action.query, destDir: action.origDir })
      }
      emitter.emit('media_items_actioned', { type: action.type, action: 'undo', query: action.query })
    }
  }

  function clear() {
    stack.value = []
  }

  return { count, record, removeRecorded, undoLast, undoAll, clear }
}
