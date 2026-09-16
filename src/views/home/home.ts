import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { callGQL, initMutation } from '@/lib/api/mutation'
import { homeStatsGQL, simsGQL, initQuery } from '@/lib/api/query'
import toast from '@/components/toaster'
import type { IHomeStats, IStorageMount, IContact, ISim } from '@/lib/interfaces'
import { useContactPicker } from '@/hooks/contact-picker'

export function useHomeData() {
  const { t } = useI18n()
  const mainStore = useMainStore()
  const { excludedDirs } = storeToRefs(mainStore)
  const { counter } = storeToRefs(useTempStore())
  const mounts = ref<IStorageMount[]>([])

  initQuery({
    handle: (data: IHomeStats, error: string) => {
      if (error) {
        toast(t(error), 'error')
      } else if (data) {
        mounts.value = data.mounts ?? []
        counter.value.messages = data.smsCount
        counter.value.contacts = data.contactCount
        counter.value.calls = data.callCount
        counter.value.videos = data.videoCount
        counter.value.images = data.imageCount
        counter.value.audios = data.audioCount
        counter.value.packages = data.packageCount
        counter.value.notes = data.noteCount
        counter.value.docs = data.docCount
        counter.value.feedEntries = data.feedEntryCount
        const vols = (data.mounts ?? []).filter((m) => (m.totalBytes ?? 0) > 0)
        counter.value.total = vols.reduce((sum, it) => sum + (it.totalBytes ?? 0), 0)
        counter.value.free = vols.reduce((sum, it) => sum + (it.freeBytes ?? 0), 0)
      }
    },
    document: homeStatsGQL,
    variables: () => {
      const parts = excludedDirs.value.map((d) => (d.includes(' ') ? `excluded_dir:"${d}"` : `excluded_dir:${d}`))
      return { mediaQuery: parts.join(' ') }
    },
  })

  return { mounts }
}

export function usePhoneAction() {
  const mainStore = useMainStore()
  const { callNumber } = storeToRefs(mainStore)
  const callNumberError = ref(false)
  const sims = ref<ISim[]>([])

  const { mutate: mutateCall, loading: callLoading } = initMutation({ document: callGQL })

  initQuery({
    document: simsGQL,
    handle(data: any, error: string) {
      if (!error) sims.value = data?.sims ?? []
    },
  })

  const {
    showContactPicker, selectedContactName, filteredContacts, contactsLoading,
    toggleContactPicker, onNumberInput, onNumberFocus, selectContactNumber, clearSelectedContact,
    getContactFullName,
  } = useContactPicker(() => callNumber.value || '')

  function pastePhoneNumber() {
    navigator.clipboard.readText().then((text) => { callNumber.value = text })
  }

  function callPhone() {
    if (!callNumber.value) { callNumberError.value = true; return }
    mutateCall({ number: callNumber.value, showDialer: false })
  }

  watch(callNumber, () => { callNumberError.value = false })

  return {
    callNumber, callNumberError, callLoading, pastePhoneNumber, callPhone,
    showContactPicker, selectedContactName, filteredContacts, contactsLoading,
    toggleContactPicker,
    onNumberInput: () => onNumberInput(callNumber.value || ''),
    onNumberFocus: () => onNumberFocus(callNumber.value || ''),
    selectContactNumber: (phone: string, contact: IContact) =>
      selectContactNumber(phone, contact, (n) => { callNumber.value = n }),
    clearSelectedContact: () => clearSelectedContact(() => { callNumber.value = '' }),
    getContactFullName,
  }
}

