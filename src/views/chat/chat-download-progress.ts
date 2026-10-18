export type ChatDownloadStatus = 'PENDING' | 'DOWNLOADING' | 'PAUSED' | 'FAILED' | 'COMPLETED' | 'CANCELED'

export interface ChatDownloadProgress {
  downloaded: number
  total: number
  speed: number
  status: ChatDownloadStatus
}

export interface ChatDownloadProgressItem {
  messageId: string
  downloaded: number
  total: number
  speed: number
  status: string
}

const ACTIVE_PRIORITY: readonly ChatDownloadStatus[] = ['DOWNLOADING', 'PAUSED', 'FAILED', 'PENDING']
const STATUSES: readonly ChatDownloadStatus[] = [...ACTIVE_PRIORITY, 'COMPLETED', 'CANCELED']

export function projectChatDownloadProgress(items: readonly ChatDownloadProgressItem[], previousIds: readonly string[] = []) {
  const groups = new Map<string, { progress: ChatDownloadProgress; statuses: Set<ChatDownloadStatus> }>()
  for (const item of items) {
    const status = item.status.toUpperCase() as ChatDownloadStatus
    if (!STATUSES.includes(status)) throw new Error(`Invalid chat download status: ${item.status}`)
    let group = groups.get(item.messageId)
    if (!group) {
      group = { progress: { downloaded: 0, total: 0, speed: 0, status: 'PENDING' }, statuses: new Set<ChatDownloadStatus>() }
      groups.set(item.messageId, group)
    }
    group.progress.downloaded += item.downloaded
    group.progress.total += item.total
    group.progress.speed += item.speed
    group.statuses.add(status)
  }
  const active: Record<string, ChatDownloadProgress> = Object.create(null)
  const finished: string[] = []
  for (const [id, group] of groups) {
    const status = ACTIVE_PRIORITY.find((status) => group.statuses.has(status))
    if (status) active[id] = { ...group.progress, status }
    else finished.push(id)
  }
  for (const id of previousIds) {
    if (!groups.has(id)) finished.push(id)
  }
  return { active, finished }
}
