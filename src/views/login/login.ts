import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import router from '@/plugins/router'
import { sha512 } from '@/lib/api/crypto'
import { getApiBaseUrl, getPendingLoginDevice, clearPendingLoginDevice } from '@/lib/api/api'
import type { InitResult } from '@/lib/api/init'
import { getCurrentAuthToken } from '@/lib/device/current'
import { findLoginPeer, saveLoginPeer, peerHost } from '@/lib/device/login-peers'
import { DeviceType } from '@/lib/status'
import { getRemoteClientId, setRemoteClientId } from '@/lib/device/client-id'
import { tauriFetch } from '@/lib/api/tauri-fetch'
import { performLoginHandshake } from '@/lib/api/login-handshake'
import { get as prefsGet } from '@/lib/prefs'

type UseLoginOptions = {
  redirectOnSuccess?: boolean
  onSuccess?: () => void | Promise<void>
}

function getSafeRedirect(redirect: unknown): string {
  const r = Array.isArray(redirect) ? redirect[0] : redirect
  if (typeof r === 'string' && r.startsWith('/') && !r.startsWith('//')) return r
  return '/'
}

export function useLogin(options: UseLoginOptions = {}) {
  const { t } = useI18n()
  const showError = ref(false)
  const webAccessDisabled = ref(true)
  const showConfirm = ref(false)
  const error = ref('')
  const showPasswordInput = ref(false)
  const password = ref('')
  const passwordError = ref('')
  const isSubmitting = ref(false)
  const redirectOnSuccess = options.redirectOnSuccess !== false
  let lastInitSignaturePublicKey = ''

  async function finishLoginSuccess() {
    if (options.onSuccess) {
      await options.onSuccess()
    }
    if (redirectOnSuccess) {
      window.location.href = getSafeRedirect(router.currentRoute.value.query['redirect'])
    }
  }

  /** Absorbs an /init result that the VIEW fetched — the request and the
   *  needsSetup routing live in LoginView, never here. Returns true when
   *  the stored token was accepted and the login already completed. */
  function applyInitResult(result: InitResult): boolean {
    if (result.status === 403) {
      showError.value = true; webAccessDisabled.value = true; error.value = 'desktop_access_disabled'; return false
    }
    webAccessDisabled.value = false
    const initData = result.data
    if (initData?.signaturePublicKey) {
      lastInitSignaturePublicKey = initData.signaturePublicKey
    }
    // The server only omits the `password` field when it accepted the token we
    // presented, so a 200 with no password means we are already authenticated.
    if (result.status === 200 && initData && !initData.password && getCurrentAuthToken()) {
      void finishLoginSuccess()
      return true
    }
    if (initData?.password) {
      password.value = initData.password
      showPasswordInput.value = false
    } else {
      showPasswordInput.value = true
    }
    return false
  }

  async function onSubmit() {
    if (!password.value?.trim()) { passwordError.value = 'valid.required'; return }
    passwordError.value = ''
    if (isSubmitting.value) return
    isSubmitting.value = true
    showError.value = false; error.value = ''

    const hash = sha512(password.value)
    const myClientId = prefsGet('client_id', '')

    try {
      const { clientId, token, signaturePublicKey } = await performLoginHandshake({
        passwordHash: hash,
        clientId: myClientId,
        storedSignaturePublicKey: findLoginPeer(getRemoteClientId())?.publicKey,
        initSignaturePublicKey: lastInitSignaturePublicKey,
        onPending: () => { showConfirm.value = true },
      })

      const current = findLoginPeer(getRemoteClientId())
      const pendingLoginDevice = getPendingLoginDevice()
      const host = pendingLoginDevice?.host || (current ? peerHost(current) : '') || window.location.host || ''
      const deviceType = pendingLoginDevice?.deviceType || DeviceType.OTHER
      if (host && clientId) {
        // Empty name: the backend keeps the stored name of an existing peer.
        await saveLoginPeer({
          clientId,
          name: '',
          host,
          token,
          signaturePublicKey,
          deviceType,
        })
        setRemoteClientId(clientId)
        clearPendingLoginDevice()
      }
      void finishLoginSuccess()
    } catch (e) {
      showError.value = true; showConfirm.value = false
      const reason = typeof e === 'string' ? e : ''
      if (!reason) {
        const hcUrl = `${getApiBaseUrl()}/health`
        const hcResp = (__IS_TAURI__ && hcUrl.startsWith('https://'))
          ? await tauriFetch(hcUrl)
          : await fetch(hcUrl)
        if (hcResp.status === 200) { error.value = 'failed_connect_ws'; return }
      }
      error.value = `login.${reason ? reason : 'failed'}`
    } finally {
      isSubmitting.value = false
    }
  }

  function cancel() {
    showConfirm.value = false; showError.value = false; isSubmitting.value = false
  }

  return {
    showError, webAccessDisabled, showConfirm, error, showPasswordInput,
    password, passwordError, isSubmitting, onSubmit, cancel, t, applyInitResult,
  }
}
