import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { callGQL, setClipGQL, initMutation } from '@/lib/api/mutation'
import { homeStatsGQL, simsGQL, initQuery, type HomeStatKey } from '@/lib/api/query'
import toast from '@/components/toaster'
import type { IHomeStats, IStorageMount, IContact, ISim } from '@/lib/interfaces'
import { useContactPicker } from '@/hooks/contact-picker'

// Counts a NAS implements; the phone serves the full set.
const NAS_HOME_STAT_KEYS: HomeStatKey[] = ['audios', 'images', 'videos']

export function useHomeData() {
  const { t } = useI18n()
  const mainStore = useMainStore()
  const tempStore = useTempStore()
  const { excludedDirs } = storeToRefs(mainStore)
  const { counter, app: appState } = storeToRefs(tempStore)
  const mounts = ref<IStorageMount[]>([])

  // The device type is only known after the first `app` query, so start with
  // a mounts-only probe and rebuild the document once it is known.
  initQuery({
    handle: (data: IHomeStats, error: string) => {
      if (error) {
        toast(t(error), 'error')
      } else if (data) {
        mounts.value = data.mounts ?? []
        if (data.smsCount !== undefined) counter.value.messages = data.smsCount
        if (data.contactCount !== undefined) counter.value.contacts = data.contactCount
        if (data.callCount !== undefined) counter.value.calls = data.callCount
        if (data.videoCount !== undefined) counter.value.videos = data.videoCount
        if (data.imageCount !== undefined) counter.value.images = data.imageCount
        if (data.audioCount !== undefined) counter.value.audios = data.audioCount
        if (data.packageCount !== undefined) counter.value.packages = data.packageCount
        if (data.noteCount !== undefined) counter.value.notes = data.noteCount
        if (data.docCount !== undefined) counter.value.docs = data.docCount
        if (data.feedEntryCount !== undefined) counter.value.feedEntries = data.feedEntryCount
        const vols = (data.mounts ?? []).filter((m) => (m.totalBytes ?? 0) > 0)
        counter.value.total = vols.reduce((sum, it) => sum + (it.totalBytes ?? 0), 0)
        counter.value.free = vols.reduce((sum, it) => sum + (it.freeBytes ?? 0), 0)
      }
    },
    document: () => {
      const deviceType = appState.value?.deviceType
      const keys: HomeStatKey[] | null = deviceType === 'NAS'
        ? NAS_HOME_STAT_KEYS
        : deviceType ? ['audios', 'images', 'videos', 'docs', 'packages', 'notes', 'feedEntries', 'messages', 'calls', 'contacts'] : null
      return homeStatsGQL(keys ?? [])
    },
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

export function useClipboardAction() {
  const clipText = ref('')
  const clipTextError = ref(false)

  const { mutate: mutateSetClip, loading: setClipLoading } = initMutation({ document: setClipGQL })

  function pasteClipboardText() {
    navigator.clipboard.readText().then((text) => { clipText.value = text })
  }

  function sendClipboard() {
    if (!clipText.value) { clipTextError.value = true; return }
    mutateSetClip({ text: clipText.value })
  }

  watch(clipText, () => { clipTextError.value = false })

  return { clipText, clipTextError, setClipLoading, pasteClipboardText, sendClipboard }
}
