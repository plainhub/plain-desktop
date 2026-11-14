import { computed, onMounted, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useTempStore } from '@/stores/temp'
import { isLocalMode } from '@/lib/device/local-mode'
import { isMacPlatform } from '@/lib/platform'
import { Capability } from '@/lib/data'
import { hasFeature } from '@/lib/feature'
import { bindGraphqlClient } from '@/lib/api/gql-client'
import { pushModal } from '@/components/modal'
import { fileIndexVolumesGQL, saveFileIndexVolume, rebuildFileIndex, type FileIndexVolume } from '@/lib/api/file-index'
import FileIndexSetupDialog from '@/components/FileIndexSetupDialog.vue'

export function useFileIndexSetup() {
  const store = useTempStore()
  const eligible = computed(() => isLocalMode() && hasFeature(Capability.MEDIA_SCAN, store.app.capabilities) && isMacPlatform())

  async function needsSetup() {
    const request = bindGraphqlClient()
    const result = await request<{ fileIndexVolumes: FileIndexVolume[] }>(fileIndexVolumesGQL, undefined)
    if (result.errors?.length) throw new Error(result.errors[0].message)
    return result.data.fileIndexVolumes.some((volume) => volume.path === '/' && volume.indexMode === 'UNCONFIGURED')
  }

  async function readSetupState(request: ReturnType<typeof bindGraphqlClient>) {
    const result = await request<{ fileIndexVolumes: FileIndexVolume[]; fileIndexScanProgress: { state: string } }>(
      'query { fileIndexVolumes { id path isAvailable indexMode includedDirs excludedDirs } fileIndexScanProgress { state } }',
      undefined
    )
    if (result.errors?.length) throw new Error(result.errors[0].message)
    const root = result.data.fileIndexVolumes.find((volume) => volume.path === '/')
    if (!root) throw new Error('Primary filesystem volume unavailable')
    return { root, scanState: result.data.fileIndexScanProgress.state }
  }

  async function startIndex(request: ReturnType<typeof bindGraphqlClient>) {
    const { root, scanState } = await readSetupState(request)
    if (root.indexMode === 'UNCONFIGURED') {
      await saveFileIndexVolume({ ...root, indexMode: 'BLOCKLIST' }, request)
      return
    }
    if (['RUNNING', 'PAUSED'].includes(scanState)) return
    await rebuildFileIndex(root.id, request)
  }

  async function dismiss(request: ReturnType<typeof bindGraphqlClient>) {
    const { root } = await readSetupState(request)
    if (root.indexMode === 'UNCONFIGURED') {
      await saveFileIndexVolume({ ...root, indexMode: 'DISABLED' }, request)
    }
  }

  function show() {
    const request = bindGraphqlClient()
    return pushModal(FileIndexSetupDialog, { onContinue: () => startIndex(request), onDismiss: () => dismiss(request) })
  }

  return { eligible, needsSetup, show }
}

export function useFileIndexSetupPrompt() {
  const { eligible, needsSetup, show } = useFileIndexSetup()
  let prompted = false
  onMounted(() => {
    if (getCurrentWindow().label !== 'main') return
    watch(
      eligible,
      async (needed) => {
        if (!needed || prompted) return
        try {
          if ((await needsSetup()) && eligible.value && !prompted) {
            prompted = true
            await show()
          }
        } catch (error) {
          console.error('Unable to show file index setup', error)
        }
      },
      { immediate: true, flush: 'post' }
    )
  })
}
