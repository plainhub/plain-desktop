export enum PeerConnectionErrorCode {
  ConnectionTimeout = 'CONNECTION_TIMEOUT',
  InvalidSession = 'INVALID_SESSION',
  ManagedSocketClosed = 'MANAGED_SOCKET_CLOSED',
  ManagedSubscriptionClosed = 'MANAGED_SUBSCRIPTION_CLOSED',
  PeerRemoved = 'PEER_REMOVED',
  ProxyAuthFailed = 'PROXY_AUTH_FAILED',
  PeerHttpsUnavailable = 'PEER_HTTPS_UNAVAILABLE',
  PeerAddressUnavailable = 'PEER_ADDRESS_UNAVAILABLE',
}

const codes = new Set<string>(Object.values(PeerConnectionErrorCode))

export function parsePeerConnectionErrorCode(value: unknown): PeerConnectionErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as PeerConnectionErrorCode : undefined
}
