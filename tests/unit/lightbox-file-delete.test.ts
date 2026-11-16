import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { useLightboxFileActions } from '@/hooks/lightbox'
import { gqlFetch } from '@/lib/api/gql-client'
import { openModal } from '@/components/modal'
import emitter from '@/plugins/eventbus'
import { DataType } from '@/lib/data'
import type { ISource } from '@/components/lightbox/types'

vi.hoisted(() => { vi.stubGlobal('__IS_TAURI__', true) })

vi.mock('@/lib/api/gql-client', async (original) => ({ ...await original<typeof import('@/lib/api/gql-client')>(), gqlFetch: vi.fn() }))
vi.mock('@/components/modal', () => ({ openModal: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/hooks/media', () => ({ useDeleteItems: () => ({ deleteItem: vi.fn() }) }))
vi.mock('@/hooks/files', () => ({ useRename: () => ({ renameItem: ref() }) }))

it('deletes the confirmed lightbox file directly without stacking a dialog', async () => {
  const item = { id: 'fixture', path: '/fixture/fixture.pdf', name: 'fixture.pdf' }
  const alternate = { id: 'other', path: '/fixture/other.pdf', name: 'other.pdf' }
  const current = ref<ISource>({ path: item.path, name: item.name, data: item, type: DataType.DOC, src: '', size: 1, durationMs: 0 })
  const listener = vi.fn()
  const { deleteFile } = useLightboxFileActions(current, vi.fn(), false, ref(false))
  vi.mocked(gqlFetch).mockResolvedValueOnce({ data: { deleteFiles: { affectedCount: 1 } } })
  emitter.on('file_deleted', listener)
  try {
    const pending = deleteFile()
    current.value = { ...current.value, ...alternate, data: alternate }
    expect(await pending).toBe(true)
    expect(gqlFetch).toHaveBeenCalledExactlyOnceWith(expect.stringContaining('mutation deleteFiles'), { paths: [item.path] }, { dedupe: false })
    expect(listener).toHaveBeenCalledExactlyOnceWith({ item })
    expect(openModal).not.toHaveBeenCalled()
  } finally {
    emitter.off('file_deleted', listener)
  }
})
