import { buildQuery, parseQuery } from '@/lib/search'
import { encodeBase64 } from '@/lib/strutil'
import type { IFilter, IFileFilter } from '@/lib/interfaces'
import { parseCurrentFields, splitOpValue } from '@/components/header-search/q'
import { useSearch as useMediaSearch } from '@/hooks/search'
import { useSearch as useFilesSearch } from '@/hooks/files'

export function useHeaderSearchQuery() {
  const { buildQ: buildMediaQ, copyFilter: copyMediaFilter, parseQ: parseMediaQ } = useMediaSearch()
  const { buildQ: buildFilesQ, parseQ: parseFilesQ } = useFilesSearch()

  function buildNextQ(currentEncodedQ: string, nextText: string, nextShowHidden: boolean) {
    const fields = parseCurrentFields(currentEncodedQ).filter((f) => f.name !== 'text' && f.name !== 'show_hidden')
    const v = nextText.trim()
    if (v) fields.push({ name: 'text', op: '', value: v })
    if (nextShowHidden) fields.push({ name: 'show_hidden', op: '', value: 'true' })
    return fields.length === 0 ? '' : encodeBase64(buildQuery(fields))
  }

  function buildNextMediaQ(currentEncodedQ: string, next: IFilter) {
    const fields = parseCurrentFields(currentEncodedQ).filter(
      (f) => !['text', 'tag_id', 'bucket_id', 'type', 'trash'].includes(f.name),
    )
    if (next.bucketId) fields.push({ name: 'bucket_id', op: '', value: next.bucketId })
    if (next.trash !== undefined) fields.push({ name: 'trash', op: '', value: next.trash ? 'true' : 'false' })
    if (next.type) fields.push({ name: 'type', op: '', value: next.type })
    for (const id of next.tagIds ?? []) fields.push({ name: 'tag_id', op: '', value: id })
    if (next.text !== undefined) {
      const v = String(next.text).trim()
      if (v) fields.push({ name: 'text', op: '', value: v })
    }
    return fields.length === 0 ? '' : encodeBase64(buildQuery(fields))
  }

  function buildNextMessagesQ(currentEncodedQ: string, mediaType: string | undefined, tagIds: string[], textValue: string) {
    const fields = parseCurrentFields(currentEncodedQ).filter((f) => !['text', 'type', 'tag_id'].includes(f.name))
    if (mediaType) fields.push({ name: 'type', op: '', value: mediaType })
    for (const id of tagIds) fields.push({ name: 'tag_id', op: '', value: id })
    const v = textValue.trim()
    if (v) fields.push({ name: 'text', op: '', value: v })
    return fields.length === 0 ? '' : encodeBase64(buildQuery(fields))
  }

  function buildNextCallsQ(currentEncodedQ: string, mediaType: string | undefined, duration: string, startTime: string, textValue: string) {
    const fields = parseCurrentFields(currentEncodedQ).filter((f) => !['text', 'type', 'duration', 'start_time'].includes(f.name))
    if (mediaType) fields.push({ name: 'type', op: '', value: mediaType })
    if (duration) {
      const { op, value } = splitOpValue(duration)
      if (value) fields.push({ name: 'duration', op, value })
    }
    if (startTime) {
      const { op, value } = splitOpValue(startTime)
      if (value) fields.push({ name: 'start_time', op, value })
    }
    const v = textValue.trim()
    if (v) fields.push({ name: 'text', op: '', value: v })
    return fields.length === 0 ? '' : encodeBase64(buildQuery(fields))
  }

  function buildNextFeedsQ(currentEncodedQ: string, feedId: string | undefined, textValue: string) {
    const fields = parseCurrentFields(currentEncodedQ).filter((f) => !['text', 'feed_id'].includes(f.name))
    if (feedId) fields.push({ name: 'feed_id', op: '', value: feedId })
    const v = textValue.trim()
    if (v) fields.push({ name: 'text', op: '', value: v })
    return fields.length === 0 ? '' : encodeBase64(buildQuery(fields))
  }

  function buildNextDocsQ(currentEncodedQ: string, fileSize: string | undefined, ext: string, textValue: string) {
    const fields = parseCurrentFields(currentEncodedQ).filter((f) => !['text', 'file_size', 'ext'].includes(f.name))
    if (fileSize) {
      const { op, value } = splitOpValue(fileSize)
      if (value) fields.push({ name: 'file_size', op, value })
    }
    if (ext) fields.push({ name: 'ext', op: '', value: ext })
    const v = textValue.trim()
    if (v) fields.push({ name: 'text', op: '', value: v })
    return fields.length === 0 ? '' : encodeBase64(buildQuery(fields))
  }

  return {
    buildMediaQ, copyMediaFilter, parseMediaQ,
    buildFilesQ, parseFilesQ,
    buildNextQ, buildNextMediaQ, buildNextMessagesQ, buildNextCallsQ, buildNextFeedsQ, buildNextDocsQ,
  }
}
