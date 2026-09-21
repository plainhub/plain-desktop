import { ref } from 'vue'
import { initLazyQuery } from '@/lib/api/query'
import { contactFragment } from '@/lib/api/fragments'
import type { IContact } from '@/lib/interfaces'
import type { ISmsConversation } from '@/lib/interfaces'
import {
  buildContactNameIndex,
  getAddressDisplayLabels,
  getConversationAddresses,
  resolveContactName,
  type ContactNameIndex,
} from '@/lib/contact/name-resolution'
import { createSharedQueryRunner } from '@/lib/shared-query-runner'

const contactsMap = ref<Map<string, string>>(new Map())
const contactIndex = ref<ContactNameIndex>({ exact: new Map(), suffix: new Map() })
let loaded = false

const allContactsGQL = `
  query allContacts {
    contacts(offset: 0, limit: 10000, query: "") {
      ...ContactFragment
    }
  }
  ${contactFragment}
`

const contactsQuery = initLazyQuery({
  handle: (data: { contacts: IContact[] }, error: string) => {
    if (error) return
    if (data) {
      contactIndex.value = buildContactNameIndex(data.contacts)
      contactsMap.value = new Map(
        [...contactIndex.value.exact].flatMap(([key, value]) => value ? [[key, value] as const] : []),
      )
      loaded = true
    }
  },
  document: allContactsGQL,
  variables: () => ({}),
})

const contactsRunner = createSharedQueryRunner((force) => contactsQuery.fetch({}, { force, latest: true }))

export function useContactName() {
  function loadContacts(force = false) {
    if (!loaded || force) {
      return contactsRunner.execute(force)
    }
    return Promise.resolve()
  }

  function getContactName(address: string): string {
    if (!address) return ''
    return resolveContactName(contactIndex.value, address)
  }

  function getDisplayName(address: string | string[]): string {
    const addresses = Array.isArray(address) ? address : [address]
    const labels = getAddressDisplayLabels(contactIndex.value, addresses)
    return labels.join(', ') || '-'
  }

  function getConversationDisplayName(conversation: ISmsConversation): string {
    return getDisplayName(getConversationAddresses(conversation))
  }

  return { loadContacts, getContactName, getDisplayName, getConversationDisplayName, contactsMap }
}
