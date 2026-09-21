import type { ISmsConversation } from '@/lib/interfaces'
import { getConversationAddresses } from '@/lib/contact/name-resolution'
import { addressesMatch } from '@/lib/sms-state-sync'

export function mergeConversationPage(
  existing: ISmsConversation[],
  incoming: ISmsConversation[],
): ISmsConversation[] {
  const result = [...existing]
  const indexById = new Map(result.map((item, index) => [item.id, index]))
  for (const item of incoming) {
    const index = indexById.get(item.id)
    if (index === undefined) {
      indexById.set(item.id, result.length)
      result.push(item)
    } else {
      result[index] = item
    }
  }
  return result
}

function unanimousObservedAddress(addresses: string[]): string {
  const observed = addresses.map((address) => address.trim()).filter(Boolean)
  const first = observed[0]
  if (!first || !observed.every((address) => addressesMatch(first, address))) return ''
  return first
}

export function resolveConversationSendAddress(
  conversation: ISmsConversation | undefined,
  observedRowAddresses: string[],
  legacySchema = false,
): string {
  if (!conversation) return ''
  const participants = getConversationAddresses(conversation)
  if (conversation.addresses) return participants.length === 1 ? participants[0] : ''
  if (!legacySchema) return ''

  const observed = unanimousObservedAddress(observedRowAddresses)
  if (participants.length !== 1 || !observed) return ''
  return addressesMatch(observed, participants[0]) ? participants[0] : ''
}
