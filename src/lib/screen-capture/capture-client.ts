import {
  CAPTURE_RESULT_AVAILABLE_EVENT,
  CAPTURE_SESSION_STARTED_EVENT,
  CAPTURE_TARGET_SESSION_ENDED_EVENT,
} from './capture-events'

export { CAPTURE_RESULT_AVAILABLE_EVENT, CAPTURE_SESSION_STARTED_EVENT }
export const CAPTURE_SESSION_ENDED_EVENT = CAPTURE_TARGET_SESSION_ENDED_EVENT

const MAX_RESULT_BYTES = 160 * 1024 * 1024
const PNG_SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10] as const
const NATIVE_PHASES = new Set(['waiting_for_overlay', 'hiding_origin', 'capturing', 'frame_available', 'awaiting_presentation', 'active', 'result_available', 'delivering', 'restoring'])
const ACK_RETRY_DELAYS_MS = [50, 200, 1000] as const

export interface CaptureEvent<T> {
  payload: T
}

export type CaptureUnlisten = () => void
export type CaptureListen = (event: string, handler: (event: CaptureEvent<unknown>) => void | Promise<void>) => Promise<CaptureUnlisten>
export type CaptureInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>

export interface CaptureResultDescriptor {
  sessionId: string
  resultId: string
  width: number
  height: number
  filename: string
  mimeType: string
  byteLen: number
}

export interface CaptureResultAvailable {
  targetToken: string
  descriptor: CaptureResultDescriptor
}

export type CaptureSessionOutcome = 'cancelled' | 'saved' | 'copied' | 'failed' | 'completed'

export interface CaptureSessionEnded {
  sessionId: string
  targetToken: string
  outcome: CaptureSessionOutcome
}

export interface CaptureSessionStarted {
  sessionId: string
  targetToken: string
}

export interface CaptureStartResponse {
  sessionId: string
  overlayGeneration: number
  phase: string
}

export interface CaptureTarget {
  windowLabel: string
  targetToken: string
}

export type CaptureConsumer = (file: File) => Promise<void>

export type CaptureClientErrorCode =
  | 'acknowledgment_failed'
  | 'capture_busy'
  | 'consumer_failed'
  | 'delivery_busy'
  | 'disposed'
  | 'invalid_result'
  | 'invalid_start'
  | 'lease_release_failed'
  | 'target_registration_failed'
  | 'target_invalidation_failed'
  | 'target_unregistration_failed'
  | 'target_unavailable'

export class CaptureClientError extends Error {
  readonly code: CaptureClientErrorCode
  override readonly cause: unknown

  constructor(code: CaptureClientErrorCode, message: string, cause?: unknown) {
    super(message)
    this.name = 'CaptureClientError'
    this.code = code
    this.cause = cause
  }
}

export interface CaptureClientDependencies {
  windowLabel: string
  listen: CaptureListen
  invoke: CaptureInvoke
  createTargetToken?: () => string
  delay?: (milliseconds: number) => Promise<void>
  onError?: (error: CaptureClientError) => void
}

export interface CaptureConsumerRegistration {
  readonly targetToken: string | null
  activate(): string
  deactivate(): void
  dispose(): void
}

export interface CaptureClient {
  registerConsumer(consumer: CaptureConsumer): CaptureConsumerRegistration
  activeTarget(): CaptureTarget | null
  activeCapture(): { sessionId: string; targetToken: string } | null
  startComposerCapture(): Promise<CaptureStartResponse>
  dispose(): void
}

interface RegistrationState {
  disposed: boolean
  token: string | null
  consumer: CaptureConsumer
}

interface FrozenTarget {
  registration: RegistrationState
  token: string
  consumer: CaptureConsumer
}

interface ActiveSession {
  sessionId: string
  target: FrozenTarget
  targetInvalidated: boolean
  deliveryCommitted: boolean
  invalidationRequested: boolean
}

interface PendingAcknowledgment {
  event: CaptureResultAvailable
  args: ResultCommandArguments
}

interface ResultCommandArguments extends Record<string, unknown> {
  sessionId: string
  resultId: string
  targetToken: string
}

interface TargetRegistryMutation {
  command: 'screen_capture_register_target' | 'screen_capture_unregister_target'
  targetToken: string
  errorCode: 'target_registration_failed' | 'target_unregistration_failed'
}

function defaultTargetToken(): string {
  const bytes = new Uint8Array(24)
  globalThis.crypto.getRandomValues(bytes)
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
}

function defaultDelay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => globalThis.setTimeout(resolve, milliseconds))
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function requireIdentifier(value: unknown, field: string): asserts value is string {
  if (typeof value !== 'string' || value.length === 0 || value.length > 256 || value.trim() !== value) {
    throw new CaptureClientError('invalid_result', `capture ${field} is invalid`)
  }
}

function requirePositiveSafeInteger(value: unknown, field: string, maximum = Number.MAX_SAFE_INTEGER): asserts value is number {
  if (!Number.isSafeInteger(value) || (value as number) <= 0 || (value as number) > maximum) {
    throw new CaptureClientError('invalid_result', `capture ${field} is invalid`)
  }
}

function validateStartResponse(value: unknown): CaptureStartResponse {
  if (!isRecord(value)) throw new CaptureClientError('invalid_start', 'native capture start returned invalid metadata')
  const { sessionId, overlayGeneration, phase } = value
  try {
    requireIdentifier(sessionId, 'session id')
    requirePositiveSafeInteger(overlayGeneration, 'overlay generation')
  } catch (error) {
    throw new CaptureClientError('invalid_start', 'native capture start returned invalid metadata', error)
  }
  if (typeof phase !== 'string' || !NATIVE_PHASES.has(phase)) {
    throw new CaptureClientError('invalid_start', 'native capture start returned an invalid phase')
  }
  return { sessionId, overlayGeneration, phase }
}

function validateResultEvent(value: unknown): CaptureResultAvailable {
  if (!isRecord(value)) throw new CaptureClientError('invalid_result', 'capture result event is invalid')
  requireIdentifier(value.targetToken, 'target token')
  if (!isRecord(value.descriptor)) throw new CaptureClientError('invalid_result', 'capture result descriptor is invalid')
  const descriptor = value.descriptor
  requireIdentifier(descriptor.sessionId, 'session id')
  requireIdentifier(descriptor.resultId, 'result id')
  requirePositiveSafeInteger(descriptor.width, 'width')
  requirePositiveSafeInteger(descriptor.height, 'height')
  requirePositiveSafeInteger(descriptor.byteLen, 'byte length', MAX_RESULT_BYTES)
  if (
    typeof descriptor.filename !== 'string' ||
    descriptor.filename.length === 0 ||
    descriptor.filename.length > 255 ||
    descriptor.filename === '.' ||
    descriptor.filename === '..' ||
    /[\\/\0]/.test(descriptor.filename) ||
    !descriptor.filename.toLowerCase().endsWith('.png')
  ) {
    throw new CaptureClientError('invalid_result', 'capture filename is invalid')
  }
  if (descriptor.mimeType !== 'image/png') {
    throw new CaptureClientError('invalid_result', 'capture result must use image/png')
  }
  return {
    targetToken: value.targetToken,
    descriptor: {
      sessionId: descriptor.sessionId,
      resultId: descriptor.resultId,
      width: descriptor.width,
      height: descriptor.height,
      filename: descriptor.filename,
      mimeType: descriptor.mimeType,
      byteLen: descriptor.byteLen,
    },
  }
}

function validateSessionEnded(value: unknown): CaptureSessionEnded {
  if (!isRecord(value)) throw new CaptureClientError('invalid_result', 'capture terminal event is invalid')
  requireIdentifier(value.sessionId, 'session id')
  requireIdentifier(value.targetToken, 'target token')
  if (!['cancelled', 'saved', 'copied', 'failed', 'completed'].includes(value.outcome as string)) {
    throw new CaptureClientError('invalid_result', 'capture terminal outcome is invalid')
  }
  return {
    sessionId: value.sessionId,
    targetToken: value.targetToken,
    outcome: value.outcome as CaptureSessionOutcome,
  }
}

function validateSessionStarted(value: unknown): CaptureSessionStarted {
  if (!isRecord(value)) throw new CaptureClientError('invalid_start', 'capture session event is invalid')
  requireIdentifier(value.sessionId, 'session id')
  requireIdentifier(value.targetToken, 'target token')
  return { sessionId: value.sessionId, targetToken: value.targetToken }
}

function validatePngBytes(value: unknown, descriptor: CaptureResultDescriptor): ArrayBuffer {
  if (!(value instanceof ArrayBuffer)) {
    throw new CaptureClientError('invalid_result', 'capture result did not use binary IPC')
  }
  if (value.byteLength !== descriptor.byteLen) {
    throw new CaptureClientError('invalid_result', 'capture result byte length does not match its descriptor')
  }
  const bytes = new Uint8Array(value)
  if (PNG_SIGNATURE.some((byte, index) => bytes[index] !== byte)) {
    throw new CaptureClientError('invalid_result', 'capture result does not have a PNG signature')
  }
  return value
}

function asClientError(error: unknown, code: CaptureClientErrorCode, message: string): CaptureClientError {
  return error instanceof CaptureClientError ? error : new CaptureClientError(code, message, error)
}

class CaptureClientImpl implements CaptureClient {
  private readonly deps: Required<Pick<CaptureClientDependencies, 'createTargetToken' | 'delay'>> & CaptureClientDependencies
  private activeRegistration: RegistrationState | null = null
  private startingTarget: FrozenTarget | null = null
  private session: ActiveSession | null = null
  private queuedResults: CaptureResultAvailable[] = []
  private queuedTerminals: CaptureSessionEnded[] = []
  private pendingAcknowledgment: PendingAcknowledgment | null = null
  private listenerPromise: Promise<void> | null = null
  private unlisteners: CaptureUnlisten[] = []
  private targetRegistryMutations: TargetRegistryMutation[] = []
  private processingTargetRegistryMutations = false
  private delivering = false
  private disposed = false

  constructor(deps: CaptureClientDependencies) {
    if (!deps.windowLabel.trim()) throw new CaptureClientError('target_unavailable', 'capture webview label is required')
    this.deps = {
      ...deps,
      createTargetToken: deps.createTargetToken ?? defaultTargetToken,
      delay: deps.delay ?? defaultDelay,
    }
  }

  registerConsumer(consumer: CaptureConsumer): CaptureConsumerRegistration {
    this.requireLive()
    const registration: RegistrationState = { consumer, disposed: false, token: null }
    return {
      get targetToken() {
        return registration.token
      },
      activate: () => {
        this.requireLive()
        if (registration.disposed) throw new CaptureClientError('target_unavailable', 'capture consumer was disposed')
        if (this.activeRegistration) this.deactivateRegistration(this.activeRegistration, false)
        const token = this.deps.createTargetToken()
        requireIdentifier(token, 'target token')
        registration.token = token
        this.activeRegistration = registration
        this.enqueueTargetRegistryMutation({
          command: 'screen_capture_register_target',
          targetToken: token,
          errorCode: 'target_registration_failed',
        })
        return token
      },
      deactivate: () => {
        this.deactivateRegistration(registration, false)
      },
      dispose: () => {
        this.deactivateRegistration(registration, true)
      },
    }
  }

  activeTarget(): CaptureTarget | null {
    const registration = this.activeRegistration
    if (!registration?.token || registration.disposed) return null
    return { windowLabel: this.deps.windowLabel, targetToken: registration.token }
  }

  activeCapture(): { sessionId: string; targetToken: string } | null {
    return this.session ? { sessionId: this.session.sessionId, targetToken: this.session.target.token } : null
  }

  async startComposerCapture(): Promise<CaptureStartResponse> {
    this.requireLive()
    if (this.startingTarget || this.session) {
      throw new CaptureClientError('capture_busy', 'this webview already owns a capture session')
    }
    const registration = this.activeRegistration
    if (!registration?.token || registration.disposed) {
      throw new CaptureClientError('target_unavailable', 'open a chat before starting a composer capture')
    }
    const frozen: FrozenTarget = {
      registration,
      token: registration.token,
      consumer: registration.consumer,
    }
    this.startingTarget = frozen
    try {
      await this.ensureListening()
      const response = validateStartResponse(
        await this.deps.invoke('screen_capture_start', {
          targetWindowLabel: this.deps.windowLabel,
          targetToken: frozen.token,
        })
      )
      this.session = {
        sessionId: response.sessionId,
        target: frozen,
        targetInvalidated: false,
        deliveryCommitted: false,
        invalidationRequested: false,
      }
      this.startingTarget = null
      const terminals = this.queuedTerminals.splice(0)
      const queued = this.queuedResults.splice(0)
      for (const terminal of terminals) this.dispatchSessionEnded(terminal)
      if (this.session?.sessionId === response.sessionId && !this.frozenTargetIsActive(frozen)) {
        this.invalidateFrozenTarget(frozen.registration)
      }
      for (const result of queued) void this.dispatchResult(result)
      return response
    } catch (error) {
      this.startingTarget = null
      this.queuedResults = []
      this.queuedTerminals = []
      throw asClientError(error, 'invalid_start', 'could not start screen capture')
    }
  }

  dispose(): void {
    if (this.disposed) return
    if (this.activeRegistration) this.deactivateRegistration(this.activeRegistration, true)
    this.disposed = true
    this.startingTarget = null
    if (!this.session?.deliveryCommitted) this.session = null
    this.queuedResults = []
    this.queuedTerminals = []
    if (!this.session?.deliveryCommitted) this.pendingAcknowledgment = null
    for (const unlisten of this.unlisteners.splice(0)) unlisten()
  }

  private requireLive(): void {
    if (this.disposed) throw new CaptureClientError('disposed', 'capture client has been disposed')
  }

  private frozenTargetIsActive(target: FrozenTarget): boolean {
    return this.activeRegistration === target.registration && !target.registration.disposed && target.registration.token === target.token
  }

  private deactivateRegistration(registration: RegistrationState, dispose: boolean): void {
    if (registration.disposed) return
    this.invalidateFrozenTarget(registration)
    const token = registration.token
    if (this.activeRegistration === registration) this.activeRegistration = null
    registration.token = null
    registration.disposed = dispose
    if (token) {
      this.enqueueTargetRegistryMutation({
        command: 'screen_capture_unregister_target',
        targetToken: token,
        errorCode: 'target_unregistration_failed',
      })
    }
  }

  private enqueueTargetRegistryMutation(mutation: TargetRegistryMutation): void {
    this.targetRegistryMutations.push(mutation)
    if (!this.processingTargetRegistryMutations) void this.processTargetRegistryMutations()
  }

  private async processTargetRegistryMutations(): Promise<void> {
    this.processingTargetRegistryMutations = true
    while (this.targetRegistryMutations.length > 0) {
      const mutation = this.targetRegistryMutations.shift()
      if (!mutation) break
      let lastError: unknown
      for (let attempt = 0; attempt < 2; attempt += 1) {
        try {
          if (mutation.command === 'screen_capture_register_target') await this.ensureListening()
          await this.deps.invoke(mutation.command, { targetToken: mutation.targetToken })
          lastError = undefined
          break
        } catch (error) {
          lastError = error
        }
      }
      if (lastError) {
        this.report(new CaptureClientError(mutation.errorCode, 'native capture target registry could not be updated', lastError))
      }
    }
    this.processingTargetRegistryMutations = false
  }

  private invalidateFrozenTarget(registration: RegistrationState): void {
    const session = this.session
    if (!session || session.target.registration !== registration || session.targetInvalidated) return
    if (session.deliveryCommitted) {
      session.invalidationRequested = true
      return
    }
    this.beginTargetInvalidation(session)
  }

  private beginTargetInvalidation(session: ActiveSession): void {
    if (session.targetInvalidated) return
    session.targetInvalidated = true
    const args = {
      sessionId: session.sessionId,
      targetToken: session.target.token,
    }
    void this.completeTargetInvalidation(session, args)
  }

  private async completeTargetInvalidation(session: ActiveSession, args: { sessionId: string; targetToken: string }): Promise<void> {
    let lastError: unknown
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        await this.deps.invoke('screen_capture_invalidate_target', args)
        if (this.session === session) {
          this.session = null
          this.pendingAcknowledgment = null
        }
        return
      } catch (error) {
        lastError = error
        // A matching terminal event may have completed the same cleanup while
        // the invalidation response was in flight. Never affect a later session.
        if (this.session !== session && !this.disposed) return
      }
    }
    this.report(new CaptureClientError('target_invalidation_failed', 'native capture target could not be invalidated', lastError))
  }

  private async ensureListening(): Promise<void> {
    if (!this.listenerPromise) {
      this.listenerPromise = (async () => {
        const installed: CaptureUnlisten[] = []
        try {
          installed.push(await this.deps.listen(CAPTURE_RESULT_AVAILABLE_EVENT, ({ payload }) => this.onResult(payload)))
          installed.push(await this.deps.listen(CAPTURE_SESSION_ENDED_EVENT, ({ payload }) => this.onSessionEnded(payload)))
          installed.push(await this.deps.listen(CAPTURE_SESSION_STARTED_EVENT, ({ payload }) => this.onSessionStarted(payload)))
          if (this.disposed) {
            for (const unlisten of installed) unlisten()
          } else {
            this.unlisteners.push(...installed)
          }
        } catch (error) {
          for (const unlisten of installed) unlisten()
          this.listenerPromise = null
          throw error
        }
      })()
    }
    await this.listenerPromise
  }

  private async onResult(payload: unknown): Promise<void> {
    if (this.disposed) return
    let result: CaptureResultAvailable
    try {
      result = validateResultEvent(payload)
    } catch (error) {
      this.report(asClientError(error, 'invalid_result', 'capture result metadata is invalid'))
      return
    }
    if (!this.session && this.startingTarget) {
      if (result.targetToken !== this.startingTarget.token) {
        this.report(new CaptureClientError('target_unavailable', 'capture result belongs to another target'))
        return
      }
      this.queuedResults.push(result)
      return
    }
    await this.dispatchResult(result)
  }

  private onSessionStarted(payload: unknown): void {
    if (this.disposed) return
    let started: CaptureSessionStarted
    try {
      started = validateSessionStarted(payload)
    } catch (error) {
      this.report(asClientError(error, 'invalid_start', 'capture session metadata is invalid'))
      return
    }

    const registration = this.activeRegistration
    if (!registration?.token || registration.disposed || registration.token !== started.targetToken) {
      this.report(new CaptureClientError('target_unavailable', 'global capture belongs to an inactive target'))
      void this.invalidateUnadoptedTarget(started)
      return
    }
    if (this.session) {
      if (this.session.sessionId === started.sessionId && this.session.target.token === started.targetToken) return
      this.report(new CaptureClientError('capture_busy', 'another capture session is already active in this webview'))
      void this.invalidateUnadoptedTarget(started)
      return
    }
    this.session = {
      sessionId: started.sessionId,
      target: {
        registration,
        token: registration.token,
        consumer: registration.consumer,
      },
      targetInvalidated: false,
      deliveryCommitted: false,
      invalidationRequested: false,
    }
  }

  private async invalidateUnadoptedTarget(started: CaptureSessionStarted): Promise<void> {
    let lastError: unknown
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        await this.deps.invoke('screen_capture_invalidate_target', {
          sessionId: started.sessionId,
          targetToken: started.targetToken,
        })
        return
      } catch (error) {
        lastError = error
      }
    }
    this.report(new CaptureClientError('target_invalidation_failed', 'inactive global capture target could not be invalidated', lastError))
  }

  private onSessionEnded(payload: unknown): void {
    if (this.disposed) return
    let terminal: CaptureSessionEnded
    try {
      terminal = validateSessionEnded(payload)
    } catch (error) {
      this.report(asClientError(error, 'invalid_result', 'capture terminal metadata is invalid'))
      return
    }
    if (!this.session && this.startingTarget) {
      if (terminal.targetToken !== this.startingTarget.token) {
        this.report(new CaptureClientError('target_unavailable', 'capture terminal event belongs to another target'))
        return
      }
      this.queuedTerminals.push(terminal)
      return
    }
    this.dispatchSessionEnded(terminal)
  }

  private dispatchSessionEnded(terminal: CaptureSessionEnded): void {
    const session = this.session
    // ACK may resolve before its best-effort native completion event arrives.
    // With no locally owned session there is no state for a late event to clear.
    if (!session) return
    if (terminal.sessionId !== session.sessionId) {
      this.report(new CaptureClientError('invalid_result', 'capture terminal event belongs to another session'))
      return
    }
    if (terminal.targetToken !== session.target.token) {
      this.report(new CaptureClientError('target_unavailable', 'capture terminal event belongs to another target'))
      return
    }
    this.session = null
    this.pendingAcknowledgment = null
  }

  private async dispatchResult(result: CaptureResultAvailable): Promise<void> {
    if (this.disposed) return
    try {
      await this.deliverResult(result)
    } catch (error) {
      this.report(asClientError(error, 'invalid_result', 'capture result delivery failed'))
    }
  }

  private async deliverResult(result: CaptureResultAvailable): Promise<void> {
    const session = this.session
    if (!session || result.descriptor.sessionId !== session.sessionId) {
      throw new CaptureClientError('invalid_result', 'capture result belongs to another session')
    }
    if (result.targetToken !== session.target.token) {
      throw new CaptureClientError('target_unavailable', 'capture result belongs to another target')
    }
    if (this.delivering) throw new CaptureClientError('delivery_busy', 'capture result delivery is already in progress')

    const args: ResultCommandArguments = {
      sessionId: result.descriptor.sessionId,
      resultId: result.descriptor.resultId,
      targetToken: result.targetToken,
    }
    if (this.pendingAcknowledgment) {
      if (!this.sameResult(this.pendingAcknowledgment.event, result)) {
        throw new CaptureClientError('invalid_result', 'capture acknowledgment belongs to another result')
      }
      await this.acknowledge(this.pendingAcknowledgment)
      return
    }
    if (!this.frozenTargetIsActive(session.target)) {
      throw new CaptureClientError('target_unavailable', 'the frozen capture target is no longer active')
    }

    this.delivering = true
    let resultTakeStarted = false
    try {
      // The invoke response can be lost after native code has acquired the
      // lease. Treat every started take as ambiguous until it is either
      // aborted or handed to the acknowledgment path.
      resultTakeStarted = true
      const raw = await this.deps.invoke('screen_capture_take_result', args)
      // Terminal cleanup is authoritative. Native may finish cancellation
      // while its already-started raw response is crossing IPC.
      if (this.session !== session) {
        resultTakeStarted = false
        return
      }
      const bytes = validatePngBytes(raw, result.descriptor)
      if (!this.frozenTargetIsActive(session.target)) {
        throw new CaptureClientError('target_unavailable', 'the frozen capture target is no longer active')
      }
      const file = new File([bytes], result.descriptor.filename, { type: result.descriptor.mimeType })
      // Crossing into the async consumer is the delivery commit boundary. Its
      // destination was frozen at capture start, so route deactivation must
      // not clear native authority while upload/send is already in flight.
      session.deliveryCommitted = true
      try {
        await session.target.consumer(file)
      } catch (error) {
        throw new CaptureClientError('consumer_failed', 'the capture consumer rejected the PNG', error)
      }
      resultTakeStarted = false
      const pending = { event: result, args }
      if (this.session === session) this.pendingAcknowledgment = pending
      await this.acknowledge(pending)
    } catch (error) {
      if (resultTakeStarted && this.session === session) await this.release(args, error)
      throw error
    } finally {
      session.deliveryCommitted = false
      this.delivering = false
      if (this.session === session && session.invalidationRequested) {
        this.beginTargetInvalidation(session)
      }
    }
  }

  private async acknowledge(pending: PendingAcknowledgment): Promise<void> {
    let lastError: unknown
    for (let attempt = 0; attempt <= ACK_RETRY_DELAYS_MS.length; attempt += 1) {
      if (attempt > 0) {
        await this.deps.delay(ACK_RETRY_DELAYS_MS[attempt - 1])
        if (this.pendingAcknowledgment !== pending || this.session?.sessionId !== pending.args.sessionId) return
      }
      try {
        await this.deps.invoke('screen_capture_ack_result', pending.args)
        if (this.pendingAcknowledgment === pending) this.pendingAcknowledgment = null
        if (this.session?.sessionId === pending.args.sessionId) this.session = null
        return
      } catch (error) {
        lastError = error
        // Native terminal cleanup may race the IPC response. It has already
        // made the bounded result unavailable, so a late rejection is inert.
        if (this.pendingAcknowledgment !== pending || this.session?.sessionId !== pending.args.sessionId) return
      }
    }
    throw new CaptureClientError('acknowledgment_failed', 'capture was consumed but could not be acknowledged', lastError)
  }

  private async release(args: ResultCommandArguments, deliveryError: unknown): Promise<void> {
    try {
      await this.deps.invoke('screen_capture_release_result', args)
    } catch (error) {
      throw new CaptureClientError('lease_release_failed', 'capture delivery failed and its lease could not be released', {
        deliveryError,
        releaseError: error,
      })
    }
  }

  private sameResult(left: CaptureResultAvailable, right: CaptureResultAvailable): boolean {
    return (
      left.targetToken === right.targetToken &&
      left.descriptor.sessionId === right.descriptor.sessionId &&
      left.descriptor.resultId === right.descriptor.resultId &&
      left.descriptor.width === right.descriptor.width &&
      left.descriptor.height === right.descriptor.height &&
      left.descriptor.filename === right.descriptor.filename &&
      left.descriptor.mimeType === right.descriptor.mimeType &&
      left.descriptor.byteLen === right.descriptor.byteLen
    )
  }

  private report(error: CaptureClientError): void {
    try {
      this.deps.onError?.(error)
    } catch {
      // Error reporting must never turn a native event into an unhandled rejection.
    }
  }
}

export function createCaptureClient(deps: CaptureClientDependencies): CaptureClient {
  return new CaptureClientImpl(deps)
}

let webviewCaptureClient: CaptureClient | null = null

/** Install once in a webview bootstrap; subsequent callers share the same client. */
export function getCaptureClient(deps?: CaptureClientDependencies): CaptureClient {
  if (!webviewCaptureClient) {
    if (!deps) throw new CaptureClientError('target_unavailable', 'capture client has not been installed in this webview')
    webviewCaptureClient = createCaptureClient(deps)
  }
  return webviewCaptureClient
}
