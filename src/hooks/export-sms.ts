import { ref, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ISms } from '@/lib/interfaces'
import { popModal } from '@/components/modal'
import { download, getFileUrlByPath } from '@/lib/api/file'
import { formatDateTime } from '@/lib/format'
import toast from '@/components/toaster'
import { smsGQL } from '@/lib/api/query'
import { gqlFetch } from '@/lib/api/gql-client'
import { ZipWriter } from '@/lib/zip-writer'

export const formats = [
  { value: 'json', labelKey: 'export_format_json' },
  { value: 'text', labelKey: 'export_format_text' },
  { value: 'csv', labelKey: 'export_format_csv' },
] as const

function isSent(item: ISms): boolean {
  return item.type === 'SENT' || item.type === 'OUTBOX'
}

function escapeCSV(value: string): string {
  return `"${(value ?? '').replace(/"/g, '""')}"`
}

export function useExportSms(props: {
  items: ISms[]
  query: string | null
  contactName: string
  urlTokenKey: Uint8Array | null
}) {
  const { t } = useI18n()

  const selectedFormat = ref('json')
  const exporting = ref(false)
  const progressText = ref('')
  let abortController: AbortController | null = null

  onUnmounted(() => { abortController?.abort() })

  function getDirection(item: ISms): string {
    return isSent(item) ? t('sent') : t('received')
  }

  function getSafeName(): string {
    return (
      (props.contactName || (props.query !== null ? 'all_sms' : 'sms'))
        .replace(/[^\w\u4e00-\u9fa5\u3040-\u30ff]/g, '_')
        .replace(/_+/g, '_')
        .replace(/^_|_$/g, '') || 'sms'
    )
  }

  async function loadMessages(): Promise<ISms[]> {
    if (props.items.length > 0) return props.items
    progressText.value = t('export_loading_messages')
    const result = await gqlFetch<{ sms: ISms[] }>(smsGQL, { offset: 0, limit: 100000, query: props.query ?? '' })
    return result?.data?.sms ?? []
  }

  function buildMessageContent(zip: ZipWriter, sorted: ISms[], attachmentPaths: Map<ISms, string[]>, format: string) {
    if (format === 'json') {
      const data = sorted.map((msg) => ({
        id: msg.id, sentAt: msg.sentAt, address: msg.address, type: msg.type,
        direction: getDirection(msg), body: msg.body ?? '',
        serviceCenter: msg.serviceCenter ?? '', subscriptionId: msg.subscriptionId,
        isMms: msg.isMms ?? false,
        attachmentFiles: (msg.attachments ?? []).map((a, index) => ({
          name: a.name, contentType: a.contentType,
          localPath: attachmentPaths.get(msg)![index],
        })),
        tags: (msg.tags ?? []).map((tag) => tag.name),
      }))
      zip.addFile(['messages.json'], JSON.stringify(data, null, 2))
    } else if (format === 'text') {
      const lines: string[] = []
      for (const msg of sorted) {
        const dir = isSent(msg) ? '→' : '←'
        lines.push(`[${formatDateTime(msg.sentAt)}] ${dir} ${msg.address}`)
        if (msg.body) lines.push(msg.body)
        for (const path of attachmentPaths.get(msg) ?? []) {
          lines.push(`[attachment: ${path}]`)
        }
        lines.push('')
      }
      zip.addFile(['messages.txt'], lines.join('\n'))
    } else if (format === 'csv') {
      const header = [escapeCSV(t('date')), escapeCSV(t('direction')), escapeCSV(t('address')), escapeCSV(t('body')), escapeCSV(t('attachments'))].join(',')
      const rows = sorted.map((msg) => {
        const attNames = (msg.attachments ?? []).map((a) => a.name || a.contentType).join('; ')
        return [escapeCSV(formatDateTime(msg.sentAt)), escapeCSV(getDirection(msg)), escapeCSV(msg.address ?? ''), escapeCSV(msg.body ?? ''), escapeCSV(attNames)].join(',')
      })
      zip.addFile(['messages.csv'], '\uFEFF' + header + '\n' + rows.join('\n'))
    }
  }

  async function fetchAttachments(zip: ZipWriter, sorted: ISms[], signal: AbortSignal) {
    const pathsByMessage = new Map<ISms, string[]>()
    zip.addDirectory(['attachments'])
    const totalAtt = sorted.reduce((s, m) => s + (m.attachments?.length ?? 0), 0)
    let fetchedAtt = 0
    for (const msg of sorted) {
      if (!msg.attachments?.length) continue
      const folder = ['attachments', msg.id] as const
      zip.addDirectory(folder)
      const paths: string[] = []
      pathsByMessage.set(msg, paths)
      for (const att of msg.attachments) {
        signal.throwIfAborted()
        const url = getFileUrlByPath(props.urlTokenKey, att.path)
        const fileName = att.name || att.path.split('/').pop() || `file_${fetchedAtt}`
        progressText.value = `${t('export_fetching_attachments')} ${fetchedAtt + 1}/${totalAtt}: ${fileName}`
        let content: Blob | undefined
        if (url) {
          try {
            const resp = await fetch(url, { signal })
            if (resp.ok) content = await resp.blob()
          } catch (e) {
            if ((e as Error)?.name === 'AbortError') throw e
          }
        }
        paths.push(zip.addFile([...folder, fileName], content))
        fetchedAtt++
      }
    }
    return pathsByMessage
  }

  async function doExport() {
    if (exporting.value) return
    const controller = new AbortController()
    abortController = controller
    const { signal } = controller
    const format = selectedFormat.value
    exporting.value = true
    try {
      const raw = await loadMessages()
      const sorted = [...raw].sort((a, b) => new Date(a.sentAt).getTime() - new Date(b.sentAt).getTime())
      signal.throwIfAborted()
      const zip = new ZipWriter()
      progressText.value = t('export_preparing_data')
      const attachmentPaths = await fetchAttachments(zip, sorted, signal)
      buildMessageContent(zip, sorted, attachmentPaths, format)
      progressText.value = t('export_generating_zip')
      const content = await zip.generate(signal)
      signal.throwIfAborted()
      const blobUrl = URL.createObjectURL(content)
      download(blobUrl, `${getSafeName()}_sms.zip`)
      setTimeout(() => URL.revokeObjectURL(blobUrl), 2000)
      popModal()
    } catch (err: any) {
      if ((err as Error)?.name !== 'AbortError') toast(err?.message ?? t('failed'), 'error')
    } finally {
      abortController = null
      exporting.value = false
      progressText.value = ''
    }
  }

  return { selectedFormat, exporting, progressText, doExport }
}
