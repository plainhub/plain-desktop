import { GraphqlErrorCode } from '@/lib/api/graphql-error-code'

export interface GraphqlFailure {
  message: string
  path?: string[]
  extensions?: { code?: string }
}

export class GqlError extends Error {
  readonly code: string

  static fromCode(code: string, status?: number, canceled = false): GqlError {
    return new GqlError(code, status, canceled, code)
  }

  static fromGraphql(error: GraphqlFailure): GqlError {
    return new GqlError(error.message, undefined, false, error.extensions?.code ?? GraphqlErrorCode.InternalError)
  }
  constructor(
    message: string,
    public status?: number,
    public canceled = false,
    code?: string,
  ) {
    super(message)
    this.name = 'GqlError'
    this.code = code ?? GraphqlErrorCode.InternalError
  }
}
