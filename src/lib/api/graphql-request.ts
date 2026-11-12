import { generateRecordId } from '../record-id.generated'

export function graphqlRequestPayload(query: string, variables?: Record<string, any>): string {
  return JSON.stringify({ requestId: generateRecordId(), query, variables })
}
