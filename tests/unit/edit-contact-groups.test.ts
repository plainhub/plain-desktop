import { expect, it, vi } from 'vitest'
import { useEditContact } from '@/hooks/edit-contact'
import type { IContact } from '@/lib/interfaces'

const update = vi.hoisted(() => vi.fn())
vi.mock('@/lib/api/mutation', () => ({
  createContactGQL: 'create',
  updateContactGQL: 'update',
  initMutation: () => ({ mutate: update, onDone: vi.fn() }),
}))
vi.mock('@/hooks/show-action-error', () => ({ showActionError: vi.fn() }))
vi.mock('@/components/PromptModal.vue', () => ({ default: {} }))
vi.mock('@/components/modal', () => ({ popModal: vi.fn(), pushModal: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

it('does not send a group change when saving an existing contact’s phone number', () => {
  const contact = {
    id: 'owned-contact', firstName: 'Fixture', middleName: '', lastName: '', prefix: '', suffix: '', notes: '',
    groups: [{ id: 'owned-group', name: 'Family', contactCount: 1 }],
    phoneNumbers: [{ value: '5550101', type: 'MOBILE', label: '' }],
    emails: [], addresses: [], websites: [], events: [], ims: [],
  } as IContact
  const form = useEditContact(contact, [], () => {})
  form.editItem.phoneNumbers[0]!.value = '5550102'
  form.doAction()

  expect(update).toHaveBeenCalledOnce()
  const [{ id, input }] = update.mock.calls[0]!
  expect(id).toBe(contact.id)
  expect(input.phoneNumbers[0].value).toBe('5550102')
  expect(input).not.toHaveProperty('groupIds')
})
