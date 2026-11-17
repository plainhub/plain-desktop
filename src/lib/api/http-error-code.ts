export enum HttpErrorCode {
  NetworkError = 'NETWORK_ERROR',
  RequestContextChanged = 'REQUEST_CONTEXT_CHANGED',
}

const codes = new Set<string>(Object.values(HttpErrorCode))

export function parseHttpErrorCode(value: unknown): HttpErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as HttpErrorCode : undefined
}
