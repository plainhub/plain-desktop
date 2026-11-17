export enum FeedOperationErrorCode {
  FeedNotFound = 'FEED_NOT_FOUND',
  InvalidFeedUrl = 'INVALID_FEED_URL',
}

const codes = new Set<string>(Object.values(FeedOperationErrorCode))

export function parseFeedOperationErrorCode(value: unknown): FeedOperationErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as FeedOperationErrorCode : undefined
}
