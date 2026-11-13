import { ref, watch } from 'vue'
import { sha256 } from '@noble/hashes/sha2.js'
import { base64ToArrayBuffer, bytesToHex } from '@/lib/strutil'
import type { BrowserIdentity } from '@/lib/device/login-peers'
import { useI18n } from 'vue-i18n'
import router from '@/plugins/router'
import { getApiBaseUrl, getPendingLoginDevice, clearPendingLoginDevice } from '@/lib/api/api'
import type { InitResult } from '@/lib/api/init'
import { getCurrentAuthToken, clearCurrentSession } from '@/lib/device/current'
import { findLoginPeer, saveBrowserSession, saveBrowserIdentity, getBrowserIdentity, replaceBrowserIdentity, forgetBrowserIdentity, preloadLoginPeers } from '@/lib/device/login-peers'
import { DeviceType } from '@/lib/status'
import { getRemoteClientId, setRemoteClientId, getDesktopClientId } from '@/lib/device/client-id'
import { loginErrorMessage } from '@/lib/api/login-error'
import { performLoginHandshake } from '@/lib/api/login-handshake'
import { bindGraphqlClient } from '@/lib/api/gql-client'
import type { SubscriptionClient } from '@/lib/api/subscription-client'
import { createPairing } from '@/lib/device/pairing'

type UseLoginOptions = {
  redirectOnSuccess?: boolean
  onSuccess?: () => void | Promise<void>
  subscriptions?: SubscriptionClient
  managedDeviceId?: string
}

export function getSafeRedirect(redirect: unknown): string {
  const value = Array.isArray(redirect) ? redirect[0] : redirect
  if (typeof value !== 'string' || !value.startsWith('/')) return '/'
  try {
    const url = new URL(value, window.location.origin)
    return url.origin === window.location.origin ? url.pathname + url.search + url.hash : '/'
  } catch { return '/' }
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
  const identityChanges = ref<{ key: string; previous: string; next: string }[]>([])
  let changedIdentity: { previous: BrowserIdentity; publicKey: string; opaquePublicKey?: string } | undefined
  let trustedIdentity: typeof changedIdentity
  let lastInitSignaturePublicKey = ''
  let lastInitOpaquePublicKey: string | undefined
  let pendingLoginController: AbortController | null = null
  const managedDeviceId = options.managedDeviceId
  const pairing = __IS_TAURI__ || managedDeviceId
    ? createPairing(options.subscriptions, managedDeviceId ? bindGraphqlClient() : undefined)
    : null
  let nativeActive = true
  watch(() => pairing?.state.value, async (state) => {
    if (!state || !nativeActive) return
    showConfirm.value = state.phase === 'PAIRING' && !!state.verificationCode
    verificationCode.value = state.verificationCode || ''
    showPasswordInput.value = state.phase === 'PASSWORD_REQUIRED' || state.phase === 'FAILED' && state.error === 'invalid_password'
    isSubmitting.value = !['PASSWORD_REQUIRED', 'SUCCEEDED', 'FAILED', 'CANCELLED'].includes(state.phase)
    if (state.phase === 'FAILED') { showError.value = true; error.value = loginErrorMessage(state.error) }
    if (state.phase === 'SUCCEEDED' && state.deviceId) {
      if (!managedDeviceId) {
        await preloadLoginPeers()
        if (!nativeActive) return
        setRemoteClientId(state.deviceId)
        clearPendingLoginDevice()
      }
      await finishLoginSuccess()
    }
  })

  async function startPairing(submittedPassword?: string) {
    if (!pairing) return
    webAccessDisabled.value = false
    isSubmitting.value = true
    showError.value = false
    error.value = ''
    const pending = getPendingLoginDevice()
    const current = findLoginPeer(getRemoteClientId())
    try {
      if (managedDeviceId) {
        await pairing.start({ peerId: managedDeviceId, mode: 'MANAGE', password: submittedPassword })
        return
      }
      const peerId = pending?.id || current?.id
      if (!peerId) throw new Error('peer_unavailable')
      await pairing.start({ peerId, mode: 'MANAGE', password: submittedPassword })
    } catch (e) { isSubmitting.value = false; showError.value = true; error.value = loginErrorMessage(e) }
  }


  async function finishLoginSuccess() {
    if (options.onSuccess) {
      await options.onSuccess()
    }
    if (redirectOnSuccess) {
      window.location.href = getSafeRedirect(router.currentRoute.value.query['redirect'])
    }
  }

  // Returns true when the stored token was accepted and login completed.
  function applyInitResult(result: InitResult): boolean {
    if (result.status === 403) {
      showError.value = true; webAccessDisabled.value = true; error.value = 'remote_access_disabled'; return false
    }
    webAccessDisabled.value = false
    const initData = result.data
    identityChanges.value = []
    changedIdentity = undefined
    lastInitOpaquePublicKey = initData?.opaquePublicKey
    if (initData?.signaturePublicKey) {
      lastInitSignaturePublicKey = initData.signaturePublicKey
    }
    if (result.status !== 200 || !initData || typeof initData.authenticated !== 'boolean' || typeof initData.passwordRequired !== 'boolean') {
      showError.value = true; webAccessDisabled.value = true; error.value = 'login.failed'; return false
    }
    showPasswordInput.value = initData.passwordRequired
    try {
      const saved = getBrowserIdentity()
      const trusted = trustedIdentity || saved
      if (trusted && (trusted.publicKey !== initData.signaturePublicKey ||
        trusted.opaquePublicKey && initData.passwordRequired && trusted.opaquePublicKey !== initData.opaquePublicKey)) {
        trustedIdentity = undefined
        if (saved) {
          const fingerprint = (key: string) => bytesToHex(sha256(base64ToArrayBuffer(key)))
          const changes = [
            { key: 'Ed25519', previous: saved.publicKey, next: initData.signaturePublicKey },
            ...(initData.passwordRequired && saved.opaquePublicKey ? [{ key: 'OPAQUE', previous: saved.opaquePublicKey, next: initData.opaquePublicKey || '' }] : []),
          ].filter(change => change.previous !== change.next)
          if (changes.every(change => base64ToArrayBuffer(change.next).length === 32)) {
            identityChanges.value = changes.map(change => ({ key: change.key, previous: fingerprint(change.previous), next: fingerprint(change.next) }))
            changedIdentity = { previous: saved, publicKey: initData.signaturePublicKey, opaquePublicKey: initData.opaquePublicKey }
          }
        }
        throw 'peer_identity_changed'
      }
    } catch {
      showError.value = true; error.value = 'login.peer_identity_changed'; webAccessDisabled.value = true; return false
    }
    if (initData.authenticated && getCurrentAuthToken()) {
      void finishLoginSuccess()
      return true
    }
    if (!initData.authenticated) void clearCurrentSession().catch(console.error)
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
        else await startPairing(showPasswordInput.value ? password.value : undefined)
        password.value = ''
      } catch (e) { isSubmitting.value = false; showError.value = true; error.value = loginErrorMessage(e) }
      return
    }
    if (showPasswordInput.value && !password.value?.trim()) { passwordError.value = 'valid.required'; return }
    passwordError.value = ''
    isSubmitting.value = true
    const controller = new AbortController()
    pendingLoginController = controller
    showError.value = false; error.value = ''

    const submittedPassword = showPasswordInput.value ? password.value : undefined
    password.value = ''
    const myClientId = getDesktopClientId()

    try {
      const approved = trustedIdentity
      const savedIdentity = getBrowserIdentity()
      const identity = approved || savedIdentity
      const { deviceId, token, signaturePublicKey, opaquePublicKey } = await performLoginHandshake({
        passwordRequired: showPasswordInput.value,
        password: submittedPassword,
        initOpaquePublicKey: lastInitOpaquePublicKey,
        storedOpaquePublicKey: identity?.opaquePublicKey,
        clientId: myClientId,
        storedSignaturePublicKey: identity?.publicKey,
        expectedDeviceId: approved ? undefined : savedIdentity?.deviceId,
        initSignaturePublicKey: lastInitSignaturePublicKey,
        onPending: (code) => { verificationCode.value = code || ''; showConfirm.value = true },
        signal: controller.signal,
      })

      if (controller.signal.aborted) return
      if (approved) {
        if (trustedIdentity !== approved) throw 'peer_identity_changed'
        replaceBrowserIdentity(approved.previous, { deviceId, publicKey: signaturePublicKey, opaquePublicKey })
        trustedIdentity = undefined
      } else {
        saveBrowserIdentity(deviceId, signaturePublicKey, opaquePublicKey)
      }
      saveBrowserSession(token)
      setRemoteClientId(deviceId)
      clearPendingLoginDevice()
      void finishLoginSuccess()
    } catch (e) {
      if (controller.signal.aborted) return
      showError.value = true; showConfirm.value = false
      error.value = loginErrorMessage(e)
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

  function trustIdentity() {
    if (!changedIdentity) return
    cancel()
    trustedIdentity = changedIdentity
    changedIdentity = undefined
    identityChanges.value = []
    showError.value = false
    webAccessDisabled.value = false
    password.value = ''
  }

  async function forgetIdentity() {
    cancel()
    forgetBrowserIdentity()
    await clearCurrentSession()
    window.location.reload()
  }

  return {
    showError, webAccessDisabled, showConfirm, verificationCode, error, showPasswordInput,
    password, passwordError, isSubmitting, onSubmit, cancel, forgetIdentity, trustIdentity, identityChanges, t, applyInitResult, startPairing,
    dispose: () => { nativeActive = false; pairing?.dispose(); pendingLoginController?.abort() },
  }
}
