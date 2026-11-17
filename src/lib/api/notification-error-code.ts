export enum NotificationErrorCode {
  ActionNotFound = 'ACTION_NOT_FOUND',
}

const codes = new Set<string>(Object.values(NotificationErrorCode))

export function parseNotificationErrorCode(value: unknown): NotificationErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as NotificationErrorCode : undefined
}
