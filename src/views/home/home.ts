import { ref, computed, watch, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useTempStore } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { useMainStore } from '@/stores/main'
import { callGQL, initMutation, pauseMediaScanGQL, resumeMediaScanGQL, stopMediaScanGQL, rebuildMediaIndexGQL } from '@/lib/api/mutation'
import { homeStatsGQL, simsGQL, initQuery, scanProgressGQL, type HomeStatKey } from '@/lib/api/query'
import toast from '@/components/toaster'
import emitter from '@/plugins/eventbus'
import type { IHomeStats, IStorageMount, IContact, ISim, IScanProgress } from '@/lib/interfaces'
import { useContactPicker } from '@/hooks/contact-picker'

// Counts a NAS implements; the phone serves the full set.
const NAS_HOME_STAT_KEYS: HomeStatKey[] = ['audios', 'images', 'videos', 'docs']

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

// Media-index scan panel on the home files card (plain-nas only): the
// `scanProgress` query seeds the initial state and the `media_scan_progress`
// WS push keeps it live; plus the pause/resume/stop/rebuild controls.
export function useScanAction() {
  const { t } = useI18n()
  const scanProgress = ref<IScanProgress>({ indexed: 0, pending: 0, total: 0, state: 'idle' })

  initQuery({
    handle: (data: { scanProgress: IScanProgress }, error: string) => {
      if (!error && data) scanProgress.value = { ...data.scanProgress }
    },
    document: scanProgressGQL,
    variables: null,
  })

  const onScanProgress = (p: IScanProgress) => {
    if (p) scanProgress.value = { indexed: p.indexed, pending: p.pending, total: p.total, state: p.state }
  }
  emitter.on('media_scan_progress', onScanProgress)
  onUnmounted(() => emitter.off('media_scan_progress', onScanProgress))

  const scanActive = computed(() => ['running', 'paused'].includes(scanProgress.value.state))

  const percent = computed(() => {
    const { indexed, total } = scanProgress.value
    if (!total) return 0
    return Math.min(100, Math.max(0, Math.round((indexed / total) * 100)))
  })

  // Backend walks the whole tree to count files before indexing starts
  // (total still 0) — show a dedicated label instead of a stuck 0%.
  const counting = computed(() => scanProgress.value.state === 'running' && scanProgress.value.total === 0)

  const stateLabel = computed(() => {
    if (scanProgress.value.state === 'running') return counting.value ? t('counting_files') : t('building_file_index')
    if (scanProgress.value.state === 'paused') return t('paused')
    if (scanProgress.value.state === 'stopped') return t('stopped')
    return ''
  })

  const showPause = computed(() => scanProgress.value.state === 'running')
  const showResume = computed(() => scanProgress.value.state === 'paused')
  const showStop = computed(() => scanActive.value)
  const showRebuild = computed(() => ['idle', 'stopped'].includes(scanProgress.value.state))

  const { mutate: pauseScanMutation } = initMutation({ document: pauseMediaScanGQL })
  const { mutate: resumeScanMutation } = initMutation({ document: resumeMediaScanGQL })
  const { mutate: stopScanMutation } = initMutation({ document: stopMediaScanGQL })
  const { mutate: rebuildIndexMutation, loading: rebuildIndexLoading } = initMutation({ document: rebuildMediaIndexGQL })

  async function pauseScan() { await pauseScanMutation() }
  async function resumeScan() { await resumeScanMutation() }
  async function stopScan() { await stopScanMutation() }
  async function rebuildIndex() { await rebuildIndexMutation({ root: '/' }) }

  return {
    scanProgress, scanActive, percent, stateLabel, counting,
    showPause, showResume, showStop, showRebuild, rebuildIndexLoading,
    pauseScan, resumeScan, stopScan, rebuildIndex,
  }
}
