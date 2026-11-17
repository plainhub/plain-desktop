import { ShareAccessErrorCode } from '@/lib/api/share-access-error-code'
import { computed, ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { guestFetch, sharedInfoGQL, sharedTokenToKey, type SharedFile, type SharedInfo } from '@/lib/api/guest'

export type ShareErrorCode =
  | 'invalid_link'
  | 'UNAUTHORIZED'
  | 'FORBIDDEN'
  | 'expired'
  | 'not_found'
  | ShareAccessErrorCode.PasswordRequired
  | 'network'
  | 'error'

function mapGraphqlError(message: string): ShareErrorCode {
  const m = message.toLowerCase()
  if (m.includes('password')) return ShareAccessErrorCode.PasswordRequired
  if (m.includes('expired') || m.includes('inactive')) return 'expired'
  if (m.includes('not found')) return 'not_found'
  if (m.includes('not allowed')) return 'FORBIDDEN'
  return 'error'
}

export function useShareData(sharedId: Ref<string>, sharedToken: Ref<string>) {
  const { t } = useI18n()

  const state = ref<'loading' | 'error' | 'ready'>('loading')
  const errorCode = ref<ShareErrorCode>('error')
  const info = ref<SharedInfo | null>(null)
  const entries = ref<SharedFile[]>([])

  const key = computed(() => {
    try {
      const value = sharedTokenToKey(sharedToken.value)
      return value.length === 32 ? value : null
    } catch {
      return null
    }
  })
  const shareTitle = computed(() => info.value?.name || t('shared_files'))
  const errorTitle = computed(() => t(`share_error_${errorCode.value}_title`))
  const errorTip = computed(() => t(`share_error_${errorCode.value}_tip`))

  async function load(relativePath: string) {
    if (!key.value) {
      errorCode.value = 'invalid_link'
      state.value = 'error'
      return
    }
    state.value = 'loading'
    try {
      const result = await guestFetch<{ sharedInfo: SharedInfo }>(
        sharedId.value,
        key.value,
        sharedInfoGQL,
        { relativePath: relativePath || null },
      )
      if (result.errors?.length) {
        errorCode.value = mapGraphqlError(result.errors[0].message)
        state.value = 'error'
        return
      }
      info.value = result.data.sharedInfo
      if (info.value.requiresPassword) {
        errorCode.value = ShareAccessErrorCode.PasswordRequired
        state.value = 'error'
        return
      }
      entries.value = info.value.entries
      state.value = 'ready'
    } catch (e: any) {
      const msg = e?.message || ''
      if (msg === 'UNAUTHORIZED') errorCode.value = 'UNAUTHORIZED'
      else if (msg === 'FORBIDDEN') errorCode.value = 'FORBIDDEN'
      else errorCode.value = 'network'
      state.value = 'error'
    }
  }

  return { state, errorCode, info, entries, shareTitle, errorTitle, errorTip, load }
}
