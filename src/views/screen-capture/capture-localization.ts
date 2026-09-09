export interface CaptureMessages {
  a11y: {
    toolbar: string
    annotationTools: string
    history: string
    colors: string
    strokeWidth: string
    actions: string
    capturedScreen: string
    annotations: string
    resizeSelection: string
    annotationText: string
    color: string
    strokeWidthOption: string
  }
  tools: {
    rectangle: string
    ellipse: string
    arrow: string
    pen: string
    text: string
    mosaic: string
  }
  actions: {
    undo: string
    redo: string
    save: string
    copy: string
    cancel: string
    confirm: string
    openChatToSend: string
  }
  status: {
    saving: string
    copying: string
    sending: string
    cancelling: string
  }
  errors: {
    saveFailed: string
    copyFailed: string
    sendFailed: string
    cancelFailed: string
  }
}

export interface CaptureLocaleModule {
  screen_capture: CaptureMessages
}

export const CAPTURE_LOCALE_CODES = ['bn', 'de', 'en-US', 'es', 'fr', 'hi', 'it', 'ja', 'ko', 'nl', 'pt', 'ru', 'ta', 'tr', 'vi', 'zh-CN', 'zh-TW'] as const

const localeModules = import.meta.glob<CaptureLocaleModule>('../../locales/*/screen-capture.ts', {
  eager: true,
  import: 'default',
})

const messagesByLocale = Object.fromEntries(
  Object.entries(localeModules).map(([path, module]) => {
    const code = path.match(/\/locales\/([^/]+)\/screen-capture\.ts$/)?.[1]
    if (!code) throw new Error(`unexpected capture locale path: ${path}`)
    return [code, module.screen_capture]
  })
) as Record<string, CaptureMessages>

export const defaultCaptureMessages = messagesByLocale['en-US']!

function preferredLocale(language: string): string | undefined {
  const normalized = language.replace('_', '-').toLowerCase()
  const exact = CAPTURE_LOCALE_CODES.find((code) => code.toLowerCase() === normalized)
  if (exact) return exact

  if (normalized.startsWith('zh-hant') || normalized === 'zh-hk' || normalized === 'zh-mo') return 'zh-TW'
  if (normalized.startsWith('zh')) return 'zh-CN'

  const base = normalized.split('-')[0]
  return CAPTURE_LOCALE_CODES.find((code) => code.toLowerCase().split('-')[0] === base)
}

export function captureMessagesForLanguages(languages: readonly string[]): CaptureMessages {
  for (const language of languages) {
    const locale = preferredLocale(language)
    if (locale) return messagesByLocale[locale] ?? defaultCaptureMessages
  }
  return defaultCaptureMessages
}

export function formatCaptureMessage(message: string, values: Readonly<Record<string, string | number>>): string {
  return message.replace(/\{([^}]+)\}/g, (placeholder, key: string) => (Object.hasOwn(values, key) ? String(values[key]) : placeholder))
}
