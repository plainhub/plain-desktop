import { PairingErrorCode, parsePairingErrorCode } from '@/lib/api/pairing-error-code'
import { GqlError } from '@/lib/api/gql-error'
import { cancelPairingGQL, pairPeerGQL } from '@/lib/api/mutation'
import { computed, reactive, ref } from 'vue'
import { nativeGraphql } from '@/lib/api/native-client'
import type { bindGraphqlClient, GqlResult } from '@/lib/api/gql-client'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'

export interface PairingView {
  deviceId: string
  phase: 'CONNECTING' | 'PAIRING' | 'PASSWORD_REQUIRED' | 'TRUST_REQUIRED' | 'SUCCEEDED' | 'FAILED' | 'CANCELED'
  error: PairingErrorCode | null
  verificationCode?: string | null
  identityChanged?: boolean
}
export interface PairingInput {
  peerId: string
  mode: 'CHAT' | 'MANAGE'
  password?: string
}
export const pairingViews = reactive(new Map<string, PairingView>())
const terminal = new Set(['SUCCEEDED', 'FAILED', 'CANCELED'])
export function isPairingActive(peerId: string) {
  const view = pairingViews.get(peerId)
  return view !== undefined && !terminal.has(view.phase)
}

export function createPairing(subscriptions = graphqlSubscriptions, request?: ReturnType<typeof bindGraphqlClient>) {
  async function execute<T>(query: string, variables?: Record<string, unknown>): Promise<T> {
    const result = request ? await request<T>(query, variables)
      : await nativeGraphql<GqlResult<T>>({ query, variables }, 30000)
    if (result.errors?.length) throw GqlError.fromGraphql(result.errors[0])
    return result.data
  }
  const peerId = ref<string>()
  const state = computed(() => peerId.value ? pairingViews.get(peerId.value) ?? null : null)
  let input: PairingInput | undefined
  let disposed = false
  const update = (id: string, patch: Partial<PairingView>) => {
    if (id !== peerId.value) return
    const current = pairingViews.get(id)
    if (current) pairingViews.set(id, { ...current, ...patch })
  }
  const stops = [
    subscriptions.subscribe('pairingStarted', result => update(result.deviceId, {
      phase: 'PAIRING', verificationCode: result.verificationCode, identityChanged: false,
    })),
    subscriptions.subscribe('pairingRequested', notice => {
      if (notice.fromId === peerId.value) update(notice.fromId, { phase: 'TRUST_REQUIRED', verificationCode: notice.verificationCode, identityChanged: notice.identityChanged })
    }),
    subscriptions.subscribe('pairingCompleted', result => update(result.deviceId, { phase: 'SUCCEEDED' })),
    subscriptions.subscribe('pairingFailed', result => {
      const error = parsePairingErrorCode(result.error) ?? PairingErrorCode.InternalError
      update(result.deviceId, {
        phase: error === PairingErrorCode.PasswordRequired || error === PairingErrorCode.InvalidPassword ? 'PASSWORD_REQUIRED' : 'FAILED', error,
      })
    }),
    subscriptions.subscribe('pairingCanceled', result => update(result.deviceId, { phase: 'CANCELED' })),
  ]
  async function cancel(id = peerId.value) {
    if (!id) return
    await execute(cancelPairingGQL, { peerId: id })
    update(id, { phase: 'CANCELED' })
  }
  async function start(value: PairingInput) {
    input = value
    peerId.value = value.peerId
    pairingViews.set(value.peerId, {
      deviceId: value.peerId, phase: value.mode === 'MANAGE' ? 'CONNECTING' : 'PAIRING', error: null,
    })
    try {
      const { peerId, ...input } = value
      await execute(pairPeerGQL, { peerId, input })
      if (disposed) await cancel(value.peerId)
    } catch (error) {
      update(value.peerId, { phase: 'FAILED', error: parsePairingErrorCode(error instanceof GqlError ? error.code : error) ?? PairingErrorCode.InternalError })
      throw error
    }
  }
  return {
    state, start, cancel,
    async confirm(id: string) {
      const requested = pairingViews.get(id)
      await execute('mutation($peerId: ID!, $accepted: Boolean!) { respondToPairing(peerId: $peerId, accepted: $accepted) }', { peerId: id, accepted: true })
      if (requested === pairingViews.get(id)) {
        update(id, {
          phase: input?.mode === 'MANAGE' ? 'CONNECTING' : 'PAIRING', verificationCode: null, identityChanged: false,
        })
      }
    },
    async submit(password: string) { if (input) await start({ ...input, password }) },
    dispose() {
      disposed = true
      stops.forEach(stop => stop())
      if (peerId.value && isPairingActive(peerId.value)) {
        void cancel(peerId.value).catch(error => console.error('Pairing cancellation failed', error))
      }
    },
  }
}
