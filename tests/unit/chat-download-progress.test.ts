import { describe, expect, it } from 'vitest'
import { projectChatDownloadProgress } from '../../src/views/chat/chat-download-progress'

const item = (status: string, messageId = 'message', downloaded = 5) => ({ messageId, status, downloaded, total: 10, speed: 2 })

describe('Rust per-file progress projection into message overlays', () => {
  it('accepts the actual lowercase wire enum and uppercase queue enum', () => {
    expect(projectChatDownloadProgress([item('downloading')])).toEqual(projectChatDownloadProgress([item('DOWNLOADING')]))
    expect(projectChatDownloadProgress([item('paused')]).active.message.status).toBe('PAUSED')
    expect(projectChatDownloadProgress([item('failed')]).active.message.status).toBe('FAILED')
  })

  it.each(['pending', 'downloading', 'paused', 'failed'])('keeps %s attachments visible when another attachment completed, in either order', (status) => {
    const active = item(status)
    const completed = item('completed', 'message', 10)
    for (const rows of [[active, completed], [completed, active]]) {
      const result = projectChatDownloadProgress(rows)
      expect(result.finished).toEqual([])
      expect(result.active.message).toEqual({ downloaded: 15, total: 20, speed: 4, status: status.toUpperCase() })
    }
  })

  it('projects the same control priority regardless of file ordering', () => {
    const rows = [item('failed'), item('pending'), item('paused'), item('downloading')]
    expect(projectChatDownloadProgress(rows)).toEqual(projectChatDownloadProgress([...rows].reverse()))
    expect(projectChatDownloadProgress(rows).active.message.status).toBe('DOWNLOADING')
    expect(projectChatDownloadProgress(rows.slice(0, 3)).active.message.status).toBe('PAUSED')
    expect(projectChatDownloadProgress(rows.slice(0, 2)).active.message.status).toBe('FAILED')
  })

  it('hides only fully terminal messages and keeps unrelated messages separate', () => {
    const result = projectChatDownloadProgress([
      item('completed', 'complete'), item('completed', 'complete'), item('canceled', 'canceled'),
      item('completed', 'mixed'), item('pending', 'mixed'), item('downloading', 'other'),
    ])
    expect(result.finished).toEqual(['complete', 'canceled'])
    expect(Object.keys(result.active)).toEqual(['mixed', 'other'])
    expect(projectChatDownloadProgress([])).toEqual({ active: {}, finished: [] })
  })

  it('treats arbitrary message IDs as data without inherited object keys', () => {
    const result = projectChatDownloadProgress([item('downloading', '__proto__'), item('paused', 'constructor')])
    expect(Object.getPrototypeOf(result.active)).toBeNull()
    expect(Object.keys(result.active)).toEqual(['__proto__', 'constructor'])
    expect(result.active['__proto__'].status).toBe('DOWNLOADING')
    expect(result.active['constructor'].status).toBe('PAUSED')
  })

  it('releases submitted-message tracking when Rust retires successful tasks', () => {
    expect(projectChatDownloadProgress([], ['committed'])).toEqual({ active: {}, finished: ['committed'] })
    const result = projectChatDownloadProgress([item('downloading', 'active')], ['committed', 'active'])
    expect(result.finished).toEqual(['committed'])
    expect(result.active.active.status).toBe('DOWNLOADING')
  })

  it('rejects invalid protocol statuses before replacing the existing UI snapshot', () => {
    expect(() => projectChatDownloadProgress([item('unknown')])).toThrow('Invalid chat download status')
  })
})
