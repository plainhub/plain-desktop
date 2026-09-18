import { ref, onActivated, onDeactivated, type Ref, type ComputedRef } from 'vue'
import toast from '@/components/toaster'
import tapPhone from '@/plugins/tapphone'
import { packageStatusesGQL, initLazyQuery } from '@/lib/api/query'
import { useI18n } from 'vue-i18n'
import type { IPackageItem, IPackageStatus } from '@/lib/interfaces'
import { initMutation, uninstallPackageGQL, installPackageGQL } from '@/lib/api/mutation'
import { useTempStore, type IUploadItem } from '@/stores/temp'
import { storeToRefs } from 'pinia'
import { useDownload, useDownloadItems } from '@/hooks/files'
import { useFileUpload, useDragDropUpload } from '@/hooks/upload'
import { deleteById } from '@/lib/array'
import emitter from '@/plugins/eventbus'
import { DataType } from '@/lib/data'
import { generateDownloadFileName } from '@/lib/format'

interface UseAppsActionsOptions {
  items: Ref<IPackageItem[]>
  isActive: Ref<boolean>
  fetch: () => void
  applyRouteQuery: () => void
  clearSelection: () => void
  pageKeyDown: (e: KeyboardEvent) => void
  pageKeyUp: (e: KeyboardEvent) => void
}

export function useAppsActions(opts: UseAppsActionsOptions) {
  const { items, isActive, fetch, applyRouteQuery, clearSelection, pageKeyDown, pageKeyUp } = opts
  const { t } = useI18n()
  const { app, urlTokenKey, uploads } = storeToRefs(useTempStore())

  const installingPackages = ref<{ id: string; updatedAt: string; isNew: boolean }[]>([])
  let statusInterval: number | undefined

  const { input: fileInput, upload: uploadFiles, uploadChanged } = useFileUpload(uploads)
  const { dropping, fileDragEnter, fileDragLeave, dropFiles } = useDragDropUpload(uploads)
  const { downloadItems: downloadItemsZip } = useDownloadItems(urlTokenKey, DataType.PACKAGE, clearSelection, () => generateDownloadFileName('apps'))
  const { downloadFile } = useDownload(urlTokenKey)

  function downloadItems(realAllChecked: boolean, ids: string[], query: string) {
    if (!realAllChecked && ids.length === 1) {
      const item = items.value.find((it) => it.id === ids[0])
      if (item) {
        downloadApp(item)
        clearSelection()
        return
      }
    }
    downloadItemsZip(realAllChecked, ids, query)
  }

  async function downloadItemsEach(ids: string[]) {
    const selected = items.value.filter((it) => ids.includes(it.id))
    for (const item of selected) {
      downloadApp(item)
      await new Promise((resolve) => setTimeout(resolve, 250))
    }
    clearSelection()
  }

  const { mutate: installPackageMutate } = initMutation({ document: installPackageGQL })
  const { mutate: uninstallMutate } = initMutation({ document: uninstallPackageGQL })

  const { loading: fetchPackageStatusLoading, fetch: fetchPackageStatus } = initLazyQuery({
    handle: (data: { packageStatuses: IPackageStatus[] }) => {
      if (data) {
        for (const item of data.packageStatuses) {
          const installingPackage = installingPackages.value.find((it) => it.id === item.id)
          if (installingPackage) {
            const isNewInstalled = installingPackage.isNew && item.exist
            const isUpgraded = !installingPackage.isNew && item.exist && installingPackage.updatedAt < item.updatedAt
            if (isNewInstalled || isUpgraded) {
              installingPackages.value = installingPackages.value.filter((it) => it.id !== item.id)
              tapPhone('')
              toast(isNewInstalled ? t('app_installation_completed') : t('app_upgrade_completed'))
              fetch()
            }
          } else if (!item.exist) {
            deleteById(items.value as any, item.id)
            tapPhone('')
          }
        }
      }
    },
    document: packageStatusesGQL,
    variables: () => ({
      ids: [...items.value.filter((it) => it.isUninstalling).map((it) => it.id), ...installingPackages.value.map((it) => it.id)],
    }),
  })

  function install() {
    uploadFiles(app.value.downloadsDir)
  }

  function uninstall(item: IPackageItem) {
    item.isUninstalling = true
    tapPhone(t('confirm_uninstallation_on_phone'))
    uninstallMutate({ id: item.id })
  }

  function cancelUninstall(item: IPackageItem) {
    item.isUninstalling = false
  }

  function downloadApp(item: IPackageItem) {
    downloadFile(item.path, `${item.name.replace(' ', '')}-${item.id}.apk`)
  }

  function dropApkFiles(e: DragEvent) {
    dropFiles(e, app.value.downloadsDir, (file) => /\.(apk|apkm|apks|xapk)$/i.test(file.name))
  }

  const uploadTaskDoneHandler = (r: IUploadItem) => {
    if (r.status === 'done') {
      installPackageMutate({ path: r.dir + '/' + r.fileName })
        .then((result) => {
          tapPhone(t('confirm_installation_on_phone'))
          if (result?.data?.installPackage) {
            const { packageName, updatedAt, isNew } = result.data.installPackage
            if (packageName) {
              installingPackages.value.push({ id: packageName, updatedAt, isNew })
              setTimeout(() => {
                if (installingPackages.value.some((it) => it.id === packageName)) {
                  installingPackages.value = installingPackages.value.filter((it) => it.id !== packageName)
                  tapPhone('')
                }
              }, 120000)
            }
          }
        })
        .catch((error) => {
          tapPhone('')
          toast(t('app_installation_failed') + ': ' + error.message, 'error')
        })
    }
  }

  onActivated(() => {
    isActive.value = true
    if (statusInterval) { clearInterval(statusInterval); statusInterval = undefined }
    statusInterval = window.setInterval(() => {
      if ((items.value.some((it) => it.isUninstalling) || installingPackages.value.length > 0) && !fetchPackageStatusLoading.value) {
        fetchPackageStatus()
      }
    }, 1000)
    applyRouteQuery()
    emitter.on('upload_task_done', uploadTaskDoneHandler)
    window.addEventListener('keydown', pageKeyDown)
    window.addEventListener('keyup', pageKeyUp)
  })

  onDeactivated(() => {
    isActive.value = false
    if (statusInterval) { clearInterval(statusInterval); statusInterval = undefined }
    emitter.off('upload_task_done', uploadTaskDoneHandler)
    window.removeEventListener('keydown', pageKeyDown)
    window.removeEventListener('keyup', pageKeyUp)
  })

  return {
    fileInput, uploadChanged, dropping, fileDragEnter, fileDragLeave,
    downloadItems, downloadItemsEach, install, uninstall, cancelUninstall, downloadApp, dropApkFiles,
  }
}
