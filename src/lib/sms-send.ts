import { HttpErrorCode } from '@/lib/api/http-error-code'
import { GqlError } from '@/lib/api/gql-error'
import { gqlFetch } from '@/lib/api/gql-client'
import { sendSmsGQL } from '@/lib/api/mutation'

export interface SmsSendInput {
  number: string
  body: string
  attachmentPaths?: string[]
  subscriptionId?: number | null
  requestId: string
}

export interface SmsSendReceipt {
  requestId: string
  requiresConfirmation: boolean
}

export async function sendSms(input: SmsSendInput): Promise<{ ok: boolean; receipt?: SmsSendReceipt; error?: string }> {
  try {
    const response = await gqlFetch<{ sendSms: SmsSendReceipt }>(sendSmsGQL, input, { dedupe: false })
    const error = response.errors?.[0]?.message
    if (error) return { ok: false, error }
    const receipt = response.data?.sendSms
    return receipt ? { ok: true, receipt } : { ok: false, error: HttpErrorCode.NetworkError }
  } catch (error: any) {
    return { ok: false, error: error instanceof GqlError ? error.message : (error.message || HttpErrorCode.NetworkError) }
  }
}
