import { computed, reactive, ref } from 'vue'
import { nativeGraphql } from '@/lib/api/native-client'
import type { bindGraphqlClient, GqlResult } from '@/lib/api/gql-client'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'

export interface PairingView {
  deviceId: string
  phase: 'PAIRING' | 'PASSWORD_REQUIRED' | 'TRUST_REQUIRED' | 'SUCCEEDED' | 'FAILED' | 'CANCELED'
  error: string | null
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
    if (result.errors?.length) throw new Error(result.errors[0].message)
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
    subscriptions.subscribe('pairingStarted', result => update(result.deviceId, { phase: 'PAIRING', verificationCode: result.verificationCode })),
    subscriptions.subscribe('pairingRequested', notice => {
      if (notice.fromId === peerId.value) update(notice.fromId, { phase: 'TRUST_REQUIRED', verificationCode: notice.verificationCode, identityChanged: notice.identityChanged })
    }),
    subscriptions.subscribe('pairingCompleted', result => update(result.deviceId, { phase: 'SUCCEEDED' })),
    subscriptions.subscribe('pairingFailed', result => update(result.deviceId, {
      phase: result.error === 'password_required' || result.error === 'invalid_password' ? 'PASSWORD_REQUIRED' : 'FAILED', error: result.error,
    })),
    subscriptions.subscribe('pairingCanceled', result => update(result.deviceId, { phase: 'CANCELED' })),
  ]
  async function cancel(id = peerId.value) {
    if (!id) return
    await execute('mutation($peerId: ID!) { cancelPairing(peerId: $peerId) }', { peerId: id })
    update(id, { phase: 'CANCELED' })
  }
  async function start(value: PairingInput) {
    input = value
    peerId.value = value.peerId
    pairingViews.set(value.peerId, { deviceId: value.peerId, phase: 'PAIRING', error: null })
    try {
      await execute('mutation($input: PairPeerInput!) { pairPeer(input: $input) }', { input: value })
      if (disposed) await cancel(value.peerId)
    } catch (error) {
      update(value.peerId, { phase: 'FAILED', error: error instanceof Error ? error.message : String(error) })
      throw error
    }
  }
  return {
    state, start, cancel,
    async confirm(id: string) {
      await execute('mutation($peerId: ID!, $accepted: Boolean!) { respondToPairing(peerId: $peerId, accepted: $accepted) }', { peerId: id, accepted: true })
      update(id, { phase: 'PAIRING' })
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
