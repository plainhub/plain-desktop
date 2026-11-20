import { GqlError } from '@/lib/api/gql-error'
import { HttpErrorCode } from '@/lib/api/http-error-code'
import emitter from '@/plugins/eventbus'

export function showActionError(error: unknown) {
  emitter.emit('toast', error instanceof GqlError ? error.message : HttpErrorCode.NetworkError)
}
