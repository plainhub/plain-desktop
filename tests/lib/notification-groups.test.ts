import { describe, expect, it } from 'vitest'
import { removeNotification, upsertNotification } from '@/lib/notification-groups'
import type { INotification } from '@/lib/interfaces'

function notif(id: string, title = 't'): INotification {
  return {
    id,
    onlyOnce: false,
    isClearable: true,
    appId: 'app',
    appName: 'App',
    time: '2026-09-06 10:00:00',
    silent: false,
    title,
    body: 'b',
    icon: '',
    actions: [],
    replyActions: [],
  }
}

describe('upsertNotification', () => {
  it('prepends a new notification', () => {
    const items = [notif('1'), notif('2')]
    const next = upsertNotification(items, notif('3'))
    expect(next.map((n) => n.id)).toEqual(['3', '1', '2'])
    expect(items.map((n) => n.id)).toEqual(['1', '2'])
  })

  it('replaces an existing notification in place', () => {
    const items = [notif('1', 'old'), notif('2')]
    const next = upsertNotification(items, notif('1', 'new'))
    expect(next.map((n) => n.id)).toEqual(['1', '2'])
    expect(next[0].title).toBe('new')
  })
})

describe('removeNotification', () => {
  it('removes only the matching id', () => {
    const items = [notif('1'), notif('2'), notif('3')]
    const next = removeNotification(items, '2')
    expect(next.map((n) => n.id)).toEqual(['1', '3'])
  })
})
