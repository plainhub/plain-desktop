import { parseQuery } from '@/lib/search'
import type { IBucket, IFeed, ITag } from '@/lib/interfaces'

// The stored search tokens keep Android's numeric type codes; map them to the
// enum-name locale keys (fall back to the raw value for anything unmapped).
const MESSAGE_TYPE_KEYS: Record<string, string> = {
  1: 'INBOX',
  2: 'SENT',
  3: 'DRAFT',
}

const CALL_TYPE_KEYS: Record<string, string> = {
  1: 'INCOMING',
  2: 'OUTGOING',
  3: 'MISSED',
}

export function formatHistoryLabel(opts: {
  decoded: string
  group: string
  t: (key: string) => string
  mediaBuckets: IBucket[]
  mediaTags: ITag[]
  feeds: IFeed[]
  messageTags?: ITag[]
}) {
  const parts: string[] = []
  const textParts: string[] = []

  for (const f of parseQuery(opts.decoded)) {
    if (f.name === 'text') {
      if (f.value) textParts.push(f.value)
      continue
    }

    if (f.name === 'show_hidden') continue

    if (f.name === 'bucket_id') {
      const b = (opts.mediaBuckets ?? []).find((it) => it.id === f.value)
      parts.push(`${opts.t('folder')}: ${b?.name ?? f.value}`)
      continue
    }

    if (f.name === 'tag_id') {
      const tagList = opts.group === 'messages' ? (opts.messageTags ?? []) : (opts.mediaTags ?? [])
      const tag = tagList.find((it) => it.id === f.value)
      parts.push(`${opts.t('tag')}: ${tag?.name ?? f.value}`)
      continue
    }

    if (opts.group === 'apps' && f.name === 'type') {
      const k = `app_type.${f.value}`
      const translated = opts.t(k)
      parts.push(`${opts.t('types')}: ${translated === k ? String(f.value ?? '') : translated}`)
      continue
    }

    if (opts.group === 'messages' && f.name === 'type') {
      const k = `message_type.${MESSAGE_TYPE_KEYS[f.value] ?? f.value}`
      const translated = opts.t(k)
      parts.push(`${opts.t('types')}: ${translated === k ? String(f.value ?? '') : translated}`)
      continue
    }

    if (opts.group === 'calls' && f.name === 'type') {
      const k = `call_type.${CALL_TYPE_KEYS[f.value] ?? f.value}`
      const translated = opts.t(k)
      parts.push(`${opts.t('types')}: ${translated === k ? String(f.value ?? '') : translated}`)
      continue
    }

    if (opts.group === 'calls' && f.name === 'duration') {
      parts.push(`${opts.t('duration')}: ${f.op}${f.value}`)
      continue
    }

    if (opts.group === 'calls' && f.name === 'start_time') {
      parts.push(`${opts.t('start_time')}: ${f.op}${f.value}`)
      continue
    }

    if (opts.group === 'feeds' && f.name === 'feed_id') {
      const feed = (opts.feeds ?? []).find((it) => it.id === f.value)
      parts.push(`${opts.t('feeds')}: ${feed?.name ?? f.value}`)
      continue
    }

    if (f.name === 'trash') {
      const v = String(f.value ?? '').toLowerCase()
      if (v === 'true') parts.push(`${opts.t('trash')}: ${opts.t('yes')}`)
      else if (v === 'false') parts.push(`${opts.t('trash')}: ${opts.t('no')}`)
      else parts.push(`${opts.t('trash')}: ${String(f.value ?? '')}`)
      continue
    }

    if (f.name === 'file_size') {
      parts.push(`${opts.t('file_size')}: ${f.op}${f.value}`)
      continue
    }

    if (f.value) parts.push(`${f.name}:${f.value}`)
  }

  return [...parts, ...textParts].join(' ').trim()
}
