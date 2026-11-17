export enum SubscriptionErrorCode {
  InvalidSubscription = 'INVALID_SUBSCRIPTION',
  SubscriptionResultTooLarge = 'SUBSCRIPTION_RESULT_TOO_LARGE',
  ResyncRequired = 'RESYNC_REQUIRED',
  SlowClient = 'SLOW_CLIENT',
}

const codes = new Set<string>(Object.values(SubscriptionErrorCode))

export function parseSubscriptionErrorCode(value: unknown): SubscriptionErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as SubscriptionErrorCode : undefined
}
