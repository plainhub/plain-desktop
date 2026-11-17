export enum GraphqlErrorCode {
  InternalError = 'INTERNAL_ERROR',
  InvalidRequest = 'INVALID_REQUEST',
}

const codes = new Set<string>(Object.values(GraphqlErrorCode))

export function parseGraphqlErrorCode(value: unknown): GraphqlErrorCode | undefined {
  return typeof value === 'string' && codes.has(value) ? value as GraphqlErrorCode : undefined
}
