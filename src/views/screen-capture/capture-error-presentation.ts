import type { ComposerTranslation } from 'vue-i18n'
import { getCurrentModal, openModal } from '@/components/modal'
import toast from '@/components/toaster'
import { CaptureClientError } from '@/lib/screen-capture/capture-client'
import { isMacPlatform } from '@/lib/platform'
import CapturePermissionModal from './CapturePermissionModal.vue'

let permissionModalOpening: Promise<unknown> | null = null

export function isScreenCapturePermissionDenied(error: unknown): boolean {
  return error instanceof CaptureClientError && error.code === 'permission_denied'
}

export async function presentCaptureError(error: unknown, t: ComposerTranslation): Promise<void> {
  if (!isMacPlatform() || !isScreenCapturePermissionDenied(error)) {
    toast(t('failed'), 'error')
    return
  }
  if (getCurrentModal()?.component === CapturePermissionModal || permissionModalOpening) return

  permissionModalOpening = openModal(CapturePermissionModal).catch(() => {
    toast(t('failed'), 'error')
  })
  try {
    await permissionModalOpening
  } finally {
    permissionModalOpening = null
  }
}
