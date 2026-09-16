export interface TimestampInfo {
  original: number | string
  formatted: string
  type: 'unix-s' | 'unix-ms' | 'iso'
}

const UNIX_SEC_MIN = 946684800
const UNIX_SEC_MAX = 4102444800
const UNIX_MS_MIN = UNIX_SEC_MIN * 1000
const UNIX_MS_MAX = UNIX_SEC_MAX * 1000
const ISO_REGEX = /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}/

export function detectTimestamp(value: unknown): TimestampInfo | null {
  if (typeof value === 'number' && Number.isFinite(value)) {
    if (value >= UNIX_SEC_MIN && value <= UNIX_SEC_MAX) {
      return { original: value, formatted: formatTimestamp(value * 1000), type: 'unix-s' }
    }
    if (value >= UNIX_MS_MIN && value <= UNIX_MS_MAX) {
      return { original: value, formatted: formatTimestamp(value), type: 'unix-ms' }
    }
  }
  if (typeof value === 'string' && ISO_REGEX.test(value)) {
    const d = new Date(value)
    if (!isNaN(d.getTime())) {
      return { original: value, formatted: formatTimestamp(d.getTime()), type: 'iso' }
    }
  }
  return null
}

function formatTimestamp(ms: number): string {
  const d = new Date(ms)
  return d.toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
}
