import { computed, ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { guestFetch, sharedInfoGQL, sharedTokenToKey, type SharedFile, type SharedInfo } from '@/lib/api/guest'

export type ShareErrorCode =
  | 'invalid_link'
  | 'unauthorized'
  | 'forbidden'
  | 'expired'
  | 'not_found'
  | 'password_required'
  | 'network'
  | 'error'

function mapGraphqlError(message: string): ShareErrorCode {
  const m = message.toLowerCase()
  if (m.includes('expired') || m.includes('inactive')) return 'expired'
  if (m.includes('not found')) return 'not_found'
  if (m.includes('not allowed')) return 'forbidden'
  return 'error'
}

export function useShareData(sharedId: Ref<string>, sharedToken: Ref<string>) {
  const { t } = useI18n()

  const state = ref<'loading' | 'error' | 'ready'>('loading')
  const errorCode = ref<ShareErrorCode>('error')
  const info = ref<SharedInfo | null>(null)
  const entries = ref<SharedFile[]>([])

  const key = computed(() => (sharedToken.value ? sharedTokenToKey(sharedToken.value) : null))
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
      console.log('[ShareView] sharedInfo:', JSON.stringify(result.data, null, 2))
      if (result.errors?.length) {
        errorCode.value = mapGraphqlError(result.errors[0].message)
        state.value = 'error'
        return
      }
      info.value = result.data.sharedInfo
      if (info.value.requiresPassword) {
        errorCode.value = 'password_required'
        state.value = 'error'
        return
      }
      entries.value = info.value.entries
      state.value = 'ready'
    } catch (e: any) {
      const msg = e?.message || ''
      if (msg === 'unauthorized') errorCode.value = 'unauthorized'
      else if (msg === 'forbidden') errorCode.value = 'forbidden'
      else errorCode.value = 'network'
      state.value = 'error'
    }
  }

  return { state, errorCode, info, entries, shareTitle, errorTitle, errorTip, load }
}
