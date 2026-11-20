import { showActionError } from '@/hooks/show-action-error'
import { rebuildFileIndexGQL } from '@/lib/api/file-index'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'
import { ref, computed, watch, onActivated, onDeactivated, onUnmounted } from 'vue'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { callGQL, initMutation, pauseFileIndexGQL, resumeFileIndexGQL, stopFileIndexGQL } from '@/lib/api/mutation'
import { homeStatsGQL, simsGQL, initQuery, fileIndexScanProgressGQL, type HomeStatKey } from '@/lib/api/query'
import { Capability } from '@/lib/data'
import type { IHomeStats, IStorageMount, IContact, ISim, IFileIndexScanProgress } from '@/lib/interfaces'
import { useContactPicker } from '@/hooks/contact-picker'

// Stat keys whose domain needs a declared capability; the rest are media
// counts every server serves.
const STAT_CAPABILITY: Partial<Record<HomeStatKey, Capability>> = {
  packages: Capability.PACKAGES,
  messages: Capability.TELEPHONY,
  calls: Capability.TELEPHONY,
  contacts: Capability.TELEPHONY,
}
const ALL_HOME_STAT_KEYS: HomeStatKey[] = ['audios', 'images', 'videos', 'docs', 'packages', 'notes', 'feedEntries', 'messages', 'calls', 'contacts']

export function useHomeData() {
  const tempStore = useTempStore()
  const { counter, app: appState } = storeToRefs(tempStore)
  const mounts = ref<IStorageMount[]>([])

  const { refetch } = initQuery({
    handle: (data: IHomeStats, error: string) => {
      if (!error && data) {
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
      const capabilities = appState.value?.capabilities
      // mounts-only probe until the app query resolves the feature list
      const keys = capabilities
        ? ALL_HOME_STAT_KEYS.filter((k) => {
            const capability = STAT_CAPABILITY[k]
            return !capability || capabilities.includes(capability)
          })
        : []
      return homeStatsGQL(keys)
    },
    variables: () => ({ mediaQuery: '' }),
  })

  let active = true
  onDeactivated(() => { active = false })
  onActivated(() => {
    if (active) return
    active = true
    void refetch(undefined, true)
  })
  const stopScanProgress = graphqlSubscriptions.subscribe('fileIndexScanProgressUpdated', (progress) => {
    if (active && progress.state === 'IDLE') return refetch(undefined, true)
  })
  onUnmounted(stopScanProgress)

  return { mounts }
}

export function usePhoneAction() {
  const mainStore = useMainStore()
  const { callNumber } = storeToRefs(mainStore)
  const callNumberError = ref(false)
  const sims = ref<ISim[]>([])

  const { mutate: mutateCall, loading: callLoading } = initMutation({ document: callGQL }, showActionError)

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

export function useScanAction(volumeId: string) {
  const fileIndexScanProgress = ref<IFileIndexScanProgress>({ volumeId, indexed: 0, pending: 0, total: 0, state: 'IDLE' })
  initQuery({
    document: fileIndexScanProgressGQL,
    variables: { volumeId },
    handle: (data: { fileIndexScanProgress: IFileIndexScanProgress }, error: string) => {
      if (!error && data) fileIndexScanProgress.value = data.fileIndexScanProgress
    },
  })
  const unsubscribe = graphqlSubscriptions.subscribe('fileIndexScanProgressUpdated', (progress: IFileIndexScanProgress) => {
    if (progress.volumeId === volumeId) fileIndexScanProgress.value = progress
  })
  onUnmounted(unsubscribe)
  const pause = initMutation({ document: pauseFileIndexGQL }, showActionError)
  const resume = initMutation({ document: resumeFileIndexGQL }, showActionError)
  const stop = initMutation({ document: stopFileIndexGQL }, showActionError)
  const rebuild = initMutation({ document: rebuildFileIndexGQL }, showActionError)
  return {
    fileIndexScanProgress,
    loading: computed(() => pause.loading.value || resume.loading.value || stop.loading.value || rebuild.loading.value),
    pause: () => pause.mutate({ volumeId }),
    resume: () => resume.mutate({ volumeId }),
    stop: () => stop.mutate({ volumeId }),
    rebuild: () => rebuild.mutate({ volumeId }),
  }
}
