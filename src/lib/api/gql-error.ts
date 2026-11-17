import { ErrorCode, parseErrorCode } from './error-code'

export interface GraphqlFailure {
  message: string
  path?: string[]
  extensions?: { code?: string }
}

export class GqlError extends Error {
  readonly code: ErrorCode

  static fromGraphql(error: GraphqlFailure): GqlError {
    return new GqlError(error.message, undefined, false, parseErrorCode(error.extensions?.code) ?? ErrorCode.InternalError)
  }
  constructor(
    message: string,
    public status?: number,
    public canceled = false,
    code?: ErrorCode,
  ) {
    super(message)
    this.name = 'GqlError'
    this.code = code ?? parseErrorCode(message) ?? ErrorCode.InternalError
  }
}
