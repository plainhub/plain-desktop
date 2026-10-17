import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import router from '@/plugins/router'
import { sha512 } from '@/lib/api/crypto'
import { getApiBaseUrl, getPendingLoginDevice, clearPendingLoginDevice } from '@/lib/api/api'
import type { InitResult } from '@/lib/api/init'
import { getCurrentAuthToken } from '@/lib/device/current'
import { findLoginPeer, saveBrowserSession, preloadLoginPeers, peerHost } from '@/lib/device/login-peers'
import { DeviceType } from '@/lib/status'
import { getRemoteClientId, setRemoteClientId } from '@/lib/device/client-id'
import { httpRequest } from '@/lib/api/http'
import { performLoginHandshake } from '@/lib/api/login-handshake'
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
    if (isSubmitting.value) return
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
    if (!password.value?.trim()) { passwordError.value = 'valid.required'; return }
    passwordError.value = ''
    isSubmitting.value = true
    const controller = new AbortController()
    pendingLoginController = controller
    showError.value = false; error.value = ''

    const hash = sha512(password.value)
    const myClientId = prefsGet('client_id', '')

    try {
      const pairingPeer = options.peer?.()
      const { clientId, token, chatPaired } = await performLoginHandshake({
        passwordHash: hash,
        clientId: myClientId,
        storedSignaturePublicKey: findLoginPeer(getRemoteClientId())?.publicKey,
        initSignaturePublicKey: lastInitSignaturePublicKey,
        onPending: () => { showConfirm.value = true },
        signal: controller.signal,
        peer: pairingPeer,
      })

      if (controller.signal.aborted) return
      if (pairingPeer && !chatPaired) throw 'chat_pairing_unavailable'

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
      error.value = `login.${reason ? reason : 'failed'}`
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

  return {
    showError, webAccessDisabled, showConfirm, error, showPasswordInput,
    password, passwordError, isSubmitting, onSubmit, cancel, t, applyInitResult, initializeNative,
    dispose: () => { nativeActive = false; pairing?.dispose(); pendingLoginController?.abort() },
  }
}
