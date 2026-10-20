import { DataType } from '@/lib/data'
import emitter from '@/plugins/eventbus'

const kinds = new Map<DataType, number>([[DataType.AUDIO, 1], [DataType.VIDEO, 2], [DataType.IMAGE, 3], [DataType.DOC, 24]])
const supported = new Set(kinds.values())

export function subscribeMediaLibraryUpdates(type: DataType, refresh: () => void): () => void {
  const kind = kinds.get(type)
  const updated = (event: unknown) => {
    if (!event || typeof event !== 'object' || !('types' in event) || !Array.isArray(event.types)) return
    if (!event.types.length || event.types.length > 4 || !event.types.every(value => supported.has(value))) return
    if (kind !== undefined && event.types.includes(kind)) refresh()
  }
  emitter.on('MEDIA_LIBRARY_UPDATED', updated)
  return () => emitter.off('MEDIA_LIBRARY_UPDATED', updated)
}
