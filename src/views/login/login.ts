import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import router from '@/plugins/router'
import { sha512 } from '@/lib/api/crypto'
import { getApiBaseUrl, getPendingLoginDevice, clearPendingLoginDevice } from '@/lib/api/api'
import type { InitResult } from '@/lib/api/init'
import { getCurrentAuthToken, clearCurrentSession } from '@/lib/device/current'
import { findLoginPeer, saveBrowserSession, saveBrowserIdentity, getBrowserIdentity, forgetBrowserIdentity, preloadLoginPeers, peerHost } from '@/lib/device/login-peers'
import { DeviceType } from '@/lib/status'
import { getRemoteClientId, setRemoteClientId } from '@/lib/device/client-id'
import { httpRequest } from '@/lib/api/http'
import { performLoginHandshake, secureLoginTransport } from '@/lib/api/login-handshake'
import type { LoginHandshakeParams } from '@/lib/api/login-handshake'
import { createPairing } from '@/lib/device/pairing'
import { get as prefsGet } from '@/lib/prefs'

type UseLoginOptions = {
  redirectOnSuccess?: boolean
  onSuccess?: () => void | Promise<void>
  peer?: () => LoginHandshakeParams['peer']
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
  const verificationCode = ref('')
  const error = ref('')
  const showPasswordInput = ref(false)
  const password = ref('')
  const passwordError = ref('')
  const isSubmitting = ref(false)
  const redirectOnSuccess = options.redirectOnSuccess !== false
  let lastInitSignaturePublicKey = ''
  let pendingLoginController: AbortController | null = null
  const pairing = __IS_TAURI__ ? createPairing() : null
  let nativeActive = true
  watch(() => pairing?.state.value, async (state) => {
    if (!state || !nativeActive) return
    showConfirm.value = state.phase === 'WAITING_CONFIRMATION'
    verificationCode.value = state.verificationCode || ''
    showPasswordInput.value = state.phase === 'PASSWORD_REQUIRED' || state.phase === 'FAILED' && state.error === 'invalid_password'
    isSubmitting.value = !['PASSWORD_REQUIRED', 'SUCCEEDED', 'FAILED', 'CANCELLED'].includes(state.phase)
    if (state.phase === 'FAILED') { showError.value = true; error.value = `login.${state.error || 'failed'}` }
    if (state.phase === 'SUCCEEDED' && state.deviceId) {
      await preloadLoginPeers()
      if (!nativeActive) return
      setRemoteClientId(state.deviceId)
      clearPendingLoginDevice()
      await finishLoginSuccess()
    }
  })

  async function initializeNative(submittedPassword?: string) {
    if (!pairing) return
    webAccessDisabled.value = false
    isSubmitting.value = true
    showError.value = false
    error.value = ''
    const pending = getPendingLoginDevice()
    const current = findLoginPeer(getRemoteClientId())
    try {
      await pairing.start({ id: pending?.id || current?.id, host: pending?.id ? pending.host : pending ? getApiBaseUrl() : current?.address?.baseUrl || (current ? peerHost(current) : undefined), mode: 'MANAGE', password: submittedPassword })
    } catch (e) { isSubmitting.value = false; showError.value = true; error.value = 'login.failed' }
  }


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
    if (!secureLoginTransport()) {
      showError.value = true; webAccessDisabled.value = true; error.value = 'login.secure_connection_required'; return false
    }
    if (initData?.signaturePublicKey) {
      lastInitSignaturePublicKey = initData.signaturePublicKey
    }
    if (result.status !== 200 || !initData || typeof initData.authenticated !== 'boolean' || typeof initData.passwordRequired !== 'boolean') {
      showError.value = true; webAccessDisabled.value = true; error.value = 'login.failed'; return false
    }
    try {
      const saved = getBrowserIdentity()
      if (saved && saved.publicKey !== initData.signaturePublicKey) throw 'peer_identity_changed'
    } catch {
      showError.value = true; error.value = 'login.peer_identity_changed'; webAccessDisabled.value = true; return false
    }
    if (initData.authenticated && getCurrentAuthToken()) {
      void finishLoginSuccess()
      return true
    }
    if (!initData.authenticated) clearCurrentSession()
    password.value = ''
    showPasswordInput.value = initData.passwordRequired
    return false
  }

  async function onSubmit() {
    if (isSubmitting.value || webAccessDisabled.value) return
    if (pairing) {
      if (showPasswordInput.value && !password.value) { passwordError.value = 'valid.required'; return }
      passwordError.value = ''
      showError.value = false
      isSubmitting.value = true
      try {
        if (pairing.state.value?.phase === 'PASSWORD_REQUIRED') await pairing.submit(password.value)
        else await initializeNative(showPasswordInput.value ? password.value : undefined)
        password.value = ''
      } catch { isSubmitting.value = false; showError.value = true; error.value = 'login.failed' }
      return
    }
    if (showPasswordInput.value && !password.value?.trim()) { passwordError.value = 'valid.required'; return }
    passwordError.value = ''
    isSubmitting.value = true
    const controller = new AbortController()
    pendingLoginController = controller
    showError.value = false; error.value = ''

    const hash = showPasswordInput.value ? sha512(password.value) : undefined
    const myClientId = prefsGet('client_id', '')

    try {
      const pairingPeer = options.peer?.()
      const identity = getBrowserIdentity()
      const { clientId, token, chatPaired, signaturePublicKey } = await performLoginHandshake({
        passwordRequired: showPasswordInput.value,
        passwordHash: hash,
        clientId: myClientId,
        storedSignaturePublicKey: identity?.publicKey,
        expectedDeviceId: identity?.deviceId,
        initSignaturePublicKey: lastInitSignaturePublicKey,
        onPending: (code) => { verificationCode.value = code || ''; showConfirm.value = true },
        signal: controller.signal,
        peer: pairingPeer,
      })

      if (controller.signal.aborted) return
      if (pairingPeer && !chatPaired) throw 'chat_pairing_unavailable'

      saveBrowserIdentity(clientId, signaturePublicKey)
      saveBrowserSession(token)
      setRemoteClientId(clientId)
      clearPendingLoginDevice()
      void finishLoginSuccess()
    } catch (e) {
      if (controller.signal.aborted) return
      showError.value = true; showConfirm.value = false
      const reason = typeof e === 'string' ? e : ''
      if (!reason) {
        const hcUrl = `${getApiBaseUrl()}/health`
        const hcResp = await httpRequest(hcUrl)
        if (hcResp.status === 200) { error.value = 'failed_connect_ws'; return }
      }
      error.value = ['desktop_access_disabled', 'failed_connect_ws'].includes(reason) ? reason : `login.${reason || 'failed'}`
    } finally {
      if (pendingLoginController === controller) pendingLoginController = null
      isSubmitting.value = false
    }
  }

  function cancel() {
    pendingLoginController?.abort()
    void pairing?.cancel()
    showConfirm.value = false; showError.value = false; isSubmitting.value = false
  }

  function forgetIdentity() {
    cancel()
    forgetBrowserIdentity()
    clearCurrentSession()
    window.location.reload()
  }

  return {
    showError, webAccessDisabled, showConfirm, verificationCode, error, showPasswordInput,
    password, passwordError, isSubmitting, onSubmit, cancel, forgetIdentity, t, applyInitResult, initializeNative,
    dispose: () => { nativeActive = false; pairing?.dispose(); pendingLoginController?.abort() },
  }
}
