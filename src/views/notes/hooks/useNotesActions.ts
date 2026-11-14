import { ref } from 'vue'
import type { INote } from '@/lib/interfaces'
import { useDelete } from '@/hooks/list'
import { deleteNotesGQL, initMutation, deleteNoteGQL } from '@/lib/api/mutation'
import { exportNotesGQL } from '@/lib/api/query'
import { gqlFetch } from '@/lib/api/gql-client'
import { useNotesRestore, useNotesTrash } from '@/hooks/notes'
import router, { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'
import { DataType } from '@/lib/data'
import type { Ref } from 'vue'
import { download } from '@/lib/api/file'
import toast from '@/components/toaster'
import { ZipWriter } from '@/lib/zip-writer'

interface UseNotesActionsOptions {
  items: Ref<INote[]>
  selectedIds: Ref<string[]>
  realAllChecked: Ref<boolean>
  q: Ref<string>
  total: Ref<number>
  clearSelection: () => void
  fetch: () => void
}

export function useNotesActions(opts: UseNotesActionsOptions) {
  const { items, selectedIds, realAllChecked, q, total, clearSelection, fetch } = opts
  const mainStore = useMainStore()
  const dataType = DataType.NOTE

  const { deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems } = useDelete(deleteNotesGQL, () => {
    clearSelection()
    fetch()
  })

  const { trashLoading, trash } = useNotesTrash(clearSelection, fetch)
  const { restoreLoading, restore } = useNotesRestore(clearSelection, fetch)

  function getQuery() {
    return realAllChecked.value ? q.value : `ids:${selectedIds.value.join(',')}`
  }

  async function exportNotes2() {
    try {
      const result = await gqlFetch<{ exportNotes: string }>(exportNotesGQL, { query: getQuery() }, { fresh: true })
      if (result.errors?.length) throw new Error(result.errors[0].message)
      const notes: INote[] = JSON.parse(result.data.exportNotes)
      const zip = new ZipWriter()
      for (const note of notes) {
        zip.addFile([`${(note.title || 'untitled').substring(0, 100)}.md`], note.content || '')
      }
      const url = URL.createObjectURL(await zip.generate())
      download(url, 'notes.zip')
      setTimeout(() => URL.revokeObjectURL(url), 2000)
    } catch (error) {
      toast(error instanceof Error ? error.message : 'failed', 'error')
    }
  }

  const { mutate: doDeleteNote, onDone: onDeleteNoteDone } = initMutation({ document: deleteNoteGQL })
  const pendingDeleteNote = ref<INote | null>(null)

  onDeleteNoteDone(() => {
    const item = pendingDeleteNote.value
    if (item) {
      items.value = items.value.filter((it) => it.id !== item.id)
      clearSelection()
      total.value--
      pendingDeleteNote.value = null
    }
  })

  function deleteItem(item: INote) {
    pendingDeleteNote.value = item
    doDeleteNote({ query: `ids:${item.id}` })
  }

  function viewUrl(item: INote) {
    const qVal = router.currentRoute.value.query.q
    return qVal ? `/notes/${item.id}?q=${qVal}` : `/notes/${item.id}`
  }

  function view(item: INote) {
    replacePath(mainStore, viewUrl(item))
  }

  function create() {
    router.push('/notes/create')
  }

  return {
    deleteItems, confirmingDelete, deleteCount, deleteLoading, doDeleteItems, cancelDeleteItems, deleteItem,
    exportNotes2, getQuery, trashLoading, trash, restoreLoading, restore,
    view, viewUrl, create,
  }
}
