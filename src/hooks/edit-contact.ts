import { reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { initMutation, createContactGQL, updateContactGQL } from '@/lib/api/mutation'
import { types } from '@/lib/contact/contact'
import { popModal, pushModal } from '@/components/modal'
import PromptModal from '@/components/PromptModal.vue'
import type {
  ContactEventType,
  EmailType,
  IContact,
  IContactAddress,
  IContactEmail,
  IContactEvent,
  IContactIm,
  IContactPhoneNumber,
  IContactWebsite,
  ImProtocol,
  PhoneType,
  PostalType,
  WebsiteType,
} from '@/lib/interfaces'

/** Shape sent as ContactInput items (no client-only fields like normalizedNumber). */
interface EditPhoneNumber { value: string; type: PhoneType; label: string }
interface EditEmail { value: string; type: EmailType; label: string }
interface EditAddress { value: string; type: PostalType; label: string }
interface EditEvent { value: string; type: ContactEventType; label: string }
interface EditWebsite { value: string; type: WebsiteType; label: string }
interface EditIm { value: string; protocol: ImProtocol; customProtocol: string }

export function useEditContact(data: IContact | undefined, sources: any[], done: () => void) {
  const { t } = useI18n()

  const editItem = reactive({
    firstName: '', middleName: '', lastName: '', prefix: '', suffix: '',
    nickname: '', organization: null as any, notes: '', source: '', starred: false,
    phoneNumbers: [] as EditPhoneNumber[],
    emails: [] as EditEmail[],
    addresses: [] as EditAddress[],
    websites: [] as EditWebsite[],
    events: [] as EditEvent[],
    ims: [] as EditIm[],
    groupIds: [] as string[],
  })

  const complexName = ref(false)
  const addFieldMenuVisible = ref(false)

  const { mutate: create, loading: createLoading, onDone: createDone } = initMutation({
    document: createContactGQL,
  })
  createDone(() => { done(); popModal() })

  const { mutate: edit, loading: editLoading, onDone: editDone } = initMutation({
    document: updateContactGQL,
  })
  editDone(() => { done(); popModal() })

  if (data) {
    Object.assign(editItem, { firstName: data.firstName, middleName: data.middleName, lastName: data.lastName, prefix: data.prefix, suffix: data.suffix, notes: data.notes })
    editItem.phoneNumbers = data.phoneNumbers.map(({ value, type, label }: IContactPhoneNumber) => ({ value, type, label }))
    editItem.emails = data.emails.map(({ value, type, label }: IContactEmail) => ({ value, type, label }))
    editItem.addresses = data.addresses.map(({ value, type, label }: IContactAddress) => ({ value, type, label }))
    editItem.websites = data.websites.map(({ value, type, label }: IContactWebsite) => ({ value, type, label }))
    editItem.events = data.events.map(({ value, type, label }: IContactEvent) => ({ value, type, label }))
    editItem.ims = data.ims.map(({ value, protocol, customProtocol }: IContactIm) => ({ value, protocol, customProtocol }))
  } else {
    editItem.phoneNumbers = [{ type: 'MOBILE', value: '', label: '' }]
  }

  const onTypeChanged = (item: { type?: string; protocol?: string; label?: string; customProtocol?: string }) => {
    const isCustom = item.type === 'CUSTOM' || item.protocol === 'CUSTOM'
    if (isCustom) {
      pushModal(PromptModal, { value: item.label || item.customProtocol, title: t('custom'), do: (value: string) => {
        if (item.type !== undefined) item.label = value
        else item.customProtocol = value
      } })
    }
  }

  const getTypeLabel = (item: { label?: string; customProtocol?: string }, type: string, key: string) => {
    return type === 'CUSTOM' ? (item.label || item.customProtocol || t('custom')) : t(`contact.${key}.${type}`)
  }

  const createTypeOptions = (typeArray: string[], key: string, item: any) => {
    return typeArray.map((type) => ({ value: type, label: getTypeLabel(item, type, key) }))
  }

  const addField = (items: any[], proto: () => any) => { items.push(proto()); addFieldMenuVisible.value = false }
  const deleteField = (items: any[], index: number) => { items.splice(index, 1) }

  function doAction() {
    if (data) { edit({ id: data.id, input: editItem }) }
    else { editItem.source = sources?.[0]?.name ?? ''; create({ input: editItem }) }
  }

  return {
    editItem, complexName, addFieldMenuVisible, createLoading, editLoading, types,
    onTypeChanged, createTypeOptions, addField, deleteField, doAction,
  }
}
