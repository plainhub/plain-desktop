import { computed, onUnmounted, ref } from 'vue'
import toast from '@/components/toaster'
import { useI18n } from 'vue-i18n'
import { initMutation, sendSmsGQL, callGQL } from '@/lib/api/mutation'
import { initQuery, simsGQL } from '@/lib/api/query'
import { upload as uploadFile } from '@/lib/upload/upload'
import { generateRecordId } from '@/lib/record-id.generated'
import tapPhone from '@/plugins/tapphone'
import type { IUploadItem } from '@/stores/temp'
import type { ISim } from '@/lib/interfaces'
import { useMainStore } from '@/stores/main'
import { sendSms } from '@/lib/sms-send'
import emitter from '@/plugins/eventbus'
import { subscribeSmsSendResults } from '@/lib/sms-result-ledger'
import { settleMmsDraft } from '@/lib/mms-draft-sync'

const MMS_WARN_SIZE = 300 * 1024

export function useMessageSend(
  appDir: () => string,
  getAddress: () => string,
  callbacks: {
    onSmsPending: (body: string, address: string) => string
    onSmsAccepted: (requestId: string) => void
    onSmsFailed: (requestId: string) => void
    onMmsPending: (id: string, body: string, address: string, attachments: { path: string; contentType: string; name: string }[]) => void
  },
) {
  const { t } = useI18n()
  const mainStore = useMainStore()
  const messageBody = ref('')
  const pendingFiles = ref<File[]>([])
  const mmsUploading = ref(false)
  const fileInputRef = ref<HTMLInputElement>()
  const sims = ref<ISim[]>([])
  const simsLoaded = ref(false)
  const selectedSimId = ref<number>(mainStore.selectedSimSubscriptionId)
  const sentMmsDrafts = new Map<string, { body: string; files: File[] }>()
  const stopResults = subscribeSmsSendResults((result) => {
    if (sentMmsDrafts.has(result.requestId)) emitter.emit('tap_phone', '')
    return false
  })
  onUnmounted(stopResults)

  initQuery({
    document: simsGQL,
    handle(data: any, error: string) {
      if (error) return
      simsLoaded.value = true
      sims.value = data?.sims ?? []
      if (selectedSimId.value === -1 && sims.value.length > 0) {
        selectedSimId.value = sims.value[0].subscriptionId
        mainStore.selectedSimSubscriptionId = selectedSimId.value
      } else if (selectedSimId.value !== -1 && !sims.value.some((s) => s.subscriptionId === selectedSimId.value)) {
        // stored SIM no longer present (SIM removed) — fall back to first
        selectedSimId.value = sims.value.length > 0 ? sims.value[0].subscriptionId : -1
        mainStore.selectedSimSubscriptionId = selectedSimId.value
      }
    },
  })

  const totalPendingSize = computed(() => pendingFiles.value.reduce((s, f) => s + f.size, 0))
  const hasLargeNonImageFile = computed(() =>
    pendingFiles.value.some((f) => !f.type.startsWith('image/') && f.size > MMS_WARN_SIZE),
  )

  const { mutate: mutateCall } = initMutation({ document: callGQL })
  const smsSending = ref(false)
  const { mutate: mutateSendMessage, loading: messageSendLoading } = initMutation({ document: sendSmsGQL })
  const invalidSelectedSim = computed(() => selectedSimId.value >= 0
    && (!simsLoaded.value || !sims.value.some((sim) => sim.subscriptionId === selectedSimId.value)))
  const sendLoading = computed(() => smsSending.value || messageSendLoading.value)
  const sendDisabled = computed(() => sendLoading.value
    || mmsUploading.value
    || (pendingFiles.value.length === 0 && invalidSelectedSim.value))

  function callContact() {
    const address = getAddress()
    if (address) mutateCall({ number: address, showDialer: false })
  }

  function openFilePicker() {
    fileInputRef.value?.click()
  }

  function onFileSelected(e: Event) {
    const input = e.target as HTMLInputElement
    if (input.files) {
      pendingFiles.value = [...pendingFiles.value, ...Array.from(input.files)]
    }
    input.value = ''
  }

  function removePendingFile(index: number) {
    pendingFiles.value = pendingFiles.value.filter((_, i) => i !== index)
  }

  async function uploadAttachments(files = pendingFiles.value): Promise<string[]> {
    const paths: string[] = []
    const mmsDir = `${appDir()}/mms_tmp`
    for (const file of files) {
      const item: IUploadItem = {
        id: generateRecordId(),
        dir: mmsDir,
        fileName: file.name,
        file,
        status: 'pending',
        uploadedSize: 0,
        error: '',
        isAppFile: false,
      }
      const result = (await uploadFile(item, false)) as { fileName?: string; error?: string } | undefined
      if (result && result.fileName) {
        paths.push(`${mmsDir}/${result.fileName}`)
      } else {
        throw new Error(t('upload_failed'))
      }
    }
    return paths
  }

  function restoreDraft(body: string) {
    const current = messageBody.value.trim()
    if (!current) messageBody.value = body
    else if (current !== body) messageBody.value = `${body}\n${messageBody.value}`
  }

  async function sendMessage() {
    const body = messageBody.value.trim()
    const address = getAddress()
    if ((!body && pendingFiles.value.length === 0) || !address || sendDisabled.value) return false

    if (pendingFiles.value.length > 0) {
      const mmsFiles = [...pendingFiles.value]
      const mmsBody = body
      const mmsAddress = address
      mmsUploading.value = true
      try {
        const attachmentPaths = await uploadAttachments(mmsFiles)
        const result = await mutateSendMessage({
          number: mmsAddress,
          body: mmsBody,
          attachmentPaths,
          requestId: generateRecordId(),
        })
        if (result == null) return false
        const receipt = result.data.sendSms
        const pendingId = receipt.requestId
        const pendingAttachments = mmsFiles.map((file) => ({
          path: URL.createObjectURL(file),
          contentType: file.type || 'application/octet-stream',
          name: file.name,
        }))
        sentMmsDrafts.set(pendingId, { body: mmsBody, files: mmsFiles })
        callbacks.onMmsPending(pendingId, mmsBody, mmsAddress, pendingAttachments)
        if (messageBody.value.trim() === mmsBody) messageBody.value = ''
        pendingFiles.value = pendingFiles.value.filter((file) => !mmsFiles.includes(file))
        if (receipt.requiresConfirmation) tapPhone(t('confirm_mms_on_phone'))
      } catch (e: any) {
        toast(e.message || t('upload_failed'), 'error')
        return false
      } finally {
        mmsUploading.value = false
      }
    } else {
      if (!body) return false
      const requestId = callbacks.onSmsPending(body, address)
      messageBody.value = ''
      smsSending.value = true
      const outcome = await sendSms({
        number: address,
        body,
        subscriptionId: selectedSimId.value,
        requestId,
      })
      smsSending.value = false
      if (!outcome.ok) {
        callbacks.onSmsFailed(requestId)
        restoreDraft(body)
        toast(t(outcome.error || 'network_error'), 'error')
        return false
      }
      callbacks.onSmsAccepted(requestId)
      emitter.emit('sms_sent')
      return true
    }
    return true
  }

  function settleMms(pendingId: string, success: boolean) {
    const outcome = settleMmsDraft(sentMmsDrafts, pendingId, success)
    if (outcome.restore) {
      restoreDraft(outcome.restore.body)
      pendingFiles.value = [
        ...outcome.restore.files,
        ...pendingFiles.value.filter((file) => !outcome.restore!.files.includes(file)),
      ]
    }
  }

  return {
    messageBody,
    pendingFiles,
    mmsUploading,
    sendLoading,
    sendDisabled,
    fileInputRef,
    totalPendingSize,
    hasLargeNonImageFile,
    MMS_WARN_SIZE,
    sims,
    selectedSimId,
    callContact,
    openFilePicker,
    onFileSelected,
    removePendingFile,
    sendMessage,
    restoreDraft,
    settleMms,
  }
}
