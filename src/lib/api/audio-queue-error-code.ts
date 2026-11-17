export enum AudioQueueErrorCode {
  BulkQueryRequired = 'BULK_QUERY_REQUIRED',
}

const codes = new Set<string>(Object.values(AudioQueueErrorCode))

export function parseAudioQueueErrorCode(value: unknown): AudioQueueErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as AudioQueueErrorCode : undefined
}
