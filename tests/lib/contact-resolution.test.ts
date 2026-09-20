import { describe, expect, it } from 'vitest'
import {
  buildContactNameIndex,
  getAddressDisplayLabels,
  getConversationAddresses,
  resolveContactName,
} from '@/lib/contact/name-resolution'
import type { IContact, ISmsConversation } from '@/lib/interfaces'

function contact(id: string, name: string, number: string): IContact {
  return {
    id, firstName: name, middleName: '', lastName: '', prefix: '', suffix: '',
    updatedAt: '', notes: '', source: '', thumbnailId: '', starred: false,
    phoneNumbers: [{ label: '', value: number, normalizedNumber: number, type: 'HOME' }],
    addresses: [], emails: [], websites: [], events: [], ims: [], tags: [],
  }
}

describe('contact name resolution', () => {
  it('does not pick an arbitrary contact when a suffix is ambiguous', () => {
    const index = buildContactNameIndex([
      contact('1', 'US Alice', '+1 202 555 0100'),
      contact('2', 'UK Alice', '+44 202 555 0100'),
    ])

    expect(resolveContactName(index, '2025550100')).toBe('')
    expect(resolveContactName(index, '+12025550100')).toBe('US Alice')
    expect(resolveContactName(index, '+442025550100')).toBe('UK Alice')
  })

  it('prefers additive participant addresses and falls back to the legacy address', () => {
    const base = { id: 't', snippet: '', date: '', messageCount: 0, read: true }
    expect(getConversationAddresses({ ...base, address: '+100', addresses: ['+200', '+300'] })).toEqual(['+200', '+300'])
    expect(getConversationAddresses({ ...base, address: '+100' } as ISmsConversation)).toEqual(['+100'])
    expect(getConversationAddresses({
      ...base,
      address: '+12025550100',
      addresses: ['+1 (202) 555-0100', '2025550100'],
    })).toEqual(['+1 (202) 555-0100'])
  })

  it('keeps one disambiguated label per distinct address when contacts share a name', () => {
    const index = buildContactNameIndex([
      contact('1', 'Alice', '+1 202 555 0100'),
      contact('2', 'Alice', '+1 202 555 0101'),
    ])
    expect(getAddressDisplayLabels(index, ['+12025550100', '+12025550101'])).toEqual([
      'Alice (+12025550100)',
      'Alice (+12025550101)',
    ])
  })
})
