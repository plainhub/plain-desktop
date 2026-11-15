import type { ComposerTranslation } from 'vue-i18n'

export function normalizeSpace(s: string) {
  return s.replace(/\s+/g, ' ').trim()
}

export function keyLabel(t: ComposerTranslation, key: string) {
  const map: Record<string, string> = {
    tag: 'tag', bucket: 'folder', type: 'type', feed_id: 'feed',
    duration: 'duration', start_time: 'start_time', history: 'search_key_history', file_size: 'file_size',
    ext: 'ext',
  }
  return map[key] ? t(map[key]) : key
}

export function keyDescription(t: ComposerTranslation, key: string) {
  const map: Record<string, string> = {
    tag: 'search_filter_by_tag', bucket: 'search_filter_by_folder',
    file_size: 'search_filter_by_file_size', duration: 'search_filter_by_duration',
    start_time: 'search_filter_by_start_time', ext: 'search_filter_by_ext',
  }
  return map[key] ? t(map[key]) : ''
}

function splitLeadingOp(raw: string) {
  const match = raw.match(/^([><=!]+)?\s*(.+)$/)
  if (!match) return { op: '', value: '' }
  return { op: match[1] || '', value: (match[2] || '').trim() }
}

function withOp(raw: string, format: (value: string) => string) {
  const { op, value } = splitLeadingOp(raw)
  if (!value) return raw
  const formatted = format(value)
  return op ? `${op} ${formatted}` : formatted
}

export function formatFileSizeFilter(raw: string) {
  return withOp(raw.trim(), value => {
    if (!/^\d+$/.test(value)) return value
    const bytes = Number(value)
    if (!Number.isSafeInteger(bytes) || bytes === 0) return value
    for (const [unit, size] of [['GB', 1024 ** 3], ['MB', 1024 ** 2], ['KB', 1024]] as const) {
      if (bytes % size === 0) return `${bytes / size}${unit}`
    }
    return value
  })
}

function humanizeSeconds(raw: string) {
  const n = raw.trim().match(/^\d+$/) ? Number(raw.trim()) : NaN
  if (!Number.isFinite(n) || n < 0) return raw
  const total = Math.floor(n)
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  if (h > 0) return s === 0 ? `${h}h ${m}m` : `${h}h ${m}m ${s}s`
  if (m > 0) return s === 0 ? `${m}m` : `${m}m ${s}s`
  return `${s}s`
}

function humanizeDateOrEpoch(raw: string) {
  const v = raw.trim()
  if (!v) return ''
  if (/^\d{4}-\d{2}-\d{2}$/.test(v)) return v
  if (/^\d+$/.test(v)) {
    const asNum = Number(v)
    if (!Number.isFinite(asNum)) return v
    const ms = v.length <= 10 ? asNum * 1000 : asNum
    const d = new Date(ms)
    if (Number.isNaN(d.getTime())) return v
    const yyyy = d.getFullYear()
    const mm = String(d.getMonth() + 1).padStart(2, '0')
    const dd = String(d.getDate()).padStart(2, '0')
    return `${yyyy}-${mm}-${dd}`
  }
  return v
}

type ValueOption = string | { value: string; label: string; description?: string }

export function resolveOptionLabel(valueOptions: Record<string, ValueOption[]>, key: string, raw: string): string {
  const options = (valueOptions ?? {})[key] ?? []
  for (const opt of options) {
    if (typeof opt === 'string') {
      if (opt === raw) return opt
      continue
    }
    if (opt.value === raw) return opt.label
  }
  return ''
}

export function displayValue(t: ComposerTranslation, valueOptions: Record<string, ValueOption[]>, key: string, raw: string) {
  const s = String(raw ?? '').trim()

  if (key === 'duration') return withOp(s, (v) => humanizeSeconds(v))
  if (key === 'start_time') return withOp(s, (v) => humanizeDateOrEpoch(v))

  if (key === 'file_size') {
    const optionLabel = resolveOptionLabel(valueOptions, key, s)
    if (optionLabel) return optionLabel
    return formatFileSizeFilter(s)
  }

  if (key === 'trash') {
    const v = s.toLowerCase()
    if (v === 'true') return t('in_trash')
    if (v === 'false') return t('not_in_trash')
    return s
  }

  const optionLabel = resolveOptionLabel(valueOptions, key, s)
  return optionLabel || s
}
