export type FileKind = 'json' | 'xml' | 'md' | 'doc' | 'txt'

export const BIG_FILE_BYTES = 8 * 1024 * 1024

const KIND_BY_EXT: Record<string, FileKind> = {}
for (const ext of ['json']) KIND_BY_EXT[ext] = 'json'
for (const ext of ['xml']) KIND_BY_EXT[ext] = 'xml'
for (const ext of ['md', 'markdown']) KIND_BY_EXT[ext] = 'md'
for (const ext of ['doc', 'docx', 'rtf', 'odt', 'pages']) KIND_BY_EXT[ext] = 'doc'

export function fileKindOf(fileName: string): FileKind {
  const name = (fileName || '').toLowerCase()
  const idx = name.lastIndexOf('.')
  const ext = idx <= 0 ? '' : name.substring(idx + 1)
  return KIND_BY_EXT[ext] || 'txt'
}

const CM_LANGUAGE_BY_KIND: Partial<Record<FileKind, string>> = {
  json: 'json',
  xml: 'xml',
  md: 'markdown',
}

export function cmLanguageFor(kind: FileKind, big: boolean): string | undefined {
  if (big) return undefined
  return CM_LANGUAGE_BY_KIND[kind]
}

export function kindBadge(kind: FileKind): string {
  switch (kind) {
    case 'json': return 'JSON'
    case 'xml': return 'XML'
    case 'md': return 'MD'
    case 'doc': return 'DOC'
    default: return 'TXT'
  }
}

export function isStructuredKind(kind: FileKind): boolean {
  return kind === 'json' || kind === 'xml'
}
