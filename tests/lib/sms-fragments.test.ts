import { describe, expect, it } from 'vitest'
import { smsConversationFragment, smsConversationWithAddressesFragment } from '../../src/lib/api/fragments'

describe('SMS conversation API fragments', () => {
  it('keeps the enhanced participant query aligned with the base schema', () => {
    const fields = (fragment: string) => fragment.slice(fragment.indexOf('{') + 1, fragment.lastIndexOf('}')).trim().split(/\s+/)
    const base = fields(smsConversationFragment)
    const enhanced = fields(smsConversationWithAddressesFragment)
    expect(base).toContain('lastMessageAt')
    expect(enhanced).toContain('addresses')
    expect(enhanced.filter(field => field !== 'addresses')).toEqual(base)
    expect(enhanced).not.toContain('date')
  })
})
