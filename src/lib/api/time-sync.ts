/**
 * Server time synchronization for anti-replay protection.
 *
 * The server injects `window.__SERVER_TIME__` into index.html on page load.
 * We compute the offset once; on localhost dev it stays 0 (same clock).
 */
import { bytesToHex } from '../strutil'

const serverTimeOffset = (window as any).__SERVER_TIME__
  ? (window as any).__SERVER_TIME__ - Date.now()
  : 0

export function getSyncedTimestamp(): number {
  if (window.__PLAIN_LOG__) console.log(`Server time offset: ${serverTimeOffset} ms`)
  return Date.now() + serverTimeOffset
}

function generateNonce(): string {
  const arr = new Uint8Array(16)
  crypto.getRandomValues(arr)
  return bytesToHex(arr)
}

/**
 * Wrap a GraphQL JSON payload with timestamp and nonce for replay protection.
 * Format: "TIMESTAMP|NONCE|JSON"
 */
export function wrapWithReplayProtection(json: string): string {
  return `${getSyncedTimestamp()}|${generateNonce()}|${json}`
}
