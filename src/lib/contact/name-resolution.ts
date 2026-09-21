import type { IContact, ISmsConversation } from '@/lib/interfaces'
import { getContactFullName } from '@/lib/contact/format'

export interface ContactNameIndex {
  exact: Map<string, string | null>
  suffix: Map<string, string | null>
}

function normalizePhone(phone: string): string {
  const trimmed = phone.trim()
  const digits = trimmed.replace(/\D/g, '')
  return trimmed.startsWith('+') && digits ? `+${digits}` : digits
}

function addressIdentity(address: string): string {
  const trimmed = address.trim()
  if (!trimmed) return ''
  const digits = trimmed.replace(/\D/g, '')
  if (!digits) return trimmed.toLowerCase()
  return digits.length === 11 && digits.startsWith('1') ? digits.slice(1) : digits
}

function addUnique(map: Map<string, string | null>, key: string, name: string) {
  if (!key) return
  const previous = map.get(key)
  if (previous === undefined) map.set(key, name)
  else if (previous !== name) map.set(key, null)
}

export function buildContactNameIndex(contacts: IContact[]): ContactNameIndex {
  const index: ContactNameIndex = { exact: new Map(), suffix: new Map() }
  for (const contact of contacts) {
    const name = getContactFullName(contact)
    if (!name) continue
    for (const phone of contact.phoneNumbers) {
      for (const raw of [phone.normalizedNumber, phone.value]) {
        if (!raw) continue
        const normalized = normalizePhone(raw)
        if (!normalized) continue
        addUnique(index.exact, normalized, name)
        const digits = normalized.replace(/\D/g, '')
        if (digits.length >= 10) addUnique(index.suffix, digits.slice(-10), name)
      }
    }
  }
  return index
}

export function resolveContactName(index: ContactNameIndex, address: string): string {
  const normalized = normalizePhone(address)
  if (!normalized) return ''
  const exact = index.exact.get(normalized)
  if (exact !== undefined) return exact ?? ''
  const digits = normalized.replace(/\D/g, '')
  if (digits.length < 10) return ''
  return index.suffix.get(digits.slice(-10)) ?? ''
}

export function getAddressDisplayLabels(index: ContactNameIndex, addresses: string[]): string[] {
  const seen = new Set<string>()
  const entries = addresses.flatMap((address) => {
    const trimmed = address.trim()
    if (!trimmed) return []
    const key = addressIdentity(trimmed)
    if (seen.has(key)) return []
    seen.add(key)
    return [{ address: trimmed, label: resolveContactName(index, trimmed) || trimmed }]
  })
  const counts = new Map<string, number>()
  for (const { label } of entries) counts.set(label, (counts.get(label) ?? 0) + 1)
  return entries.map(({ address, label }) => counts.get(label)! > 1 ? `${label} (${address})` : label)
}

export function getConversationAddresses(conversation: ISmsConversation): string[] {
  const addresses = conversation.addresses?.map((item) => item.trim()).filter(Boolean) ?? []
  const unique = new Map<string, string>()
  for (const address of addresses.length ? addresses : [conversation.address].filter(Boolean)) {
    const key = addressIdentity(address)
    if (key && !unique.has(key)) unique.set(key, address)
  }
  return [...unique.values()]
}
