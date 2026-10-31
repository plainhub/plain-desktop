import {
  cancelPairingGQL,
  initMutation,
  pairDeviceGQL,
  respondToPairingGQL,
} from '@/lib/api/mutation'
import type { PairingState } from '@/lib/device/pairing'
import type { PairingRequest } from '@/lib/pairing-types'
import { getGraphqlContextKey, gqlFetch } from '@/lib/api/gql-client'
import { onBeforeUnmount, onMounted, reactive, ref } from 'vue'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'

export type { PairingRequest }
export enum PairingStatus {
  IDLE,
  REQUESTING,
  WAITING,
  SUCCESS,
  FAILED,
  CANCELLED,
}

export enum DeviceState {
  UNPAIRED,
  PAIRING,
  PAIRED,
  UNPAIRING,
  CANCELING,
}
const deviceStates = reactive(new Map<string, DeviceState>())
const { mutate: pairDeviceMutate } = initMutation({ document: pairDeviceGQL }, false)
const { mutate: cancelPairingMutate } = initMutation({ document: cancelPairingGQL }, false)
const { mutate: respondToPairingMutate } = initMutation({ document: respondToPairingGQL })

export async function allowPairing(request: PairingRequest) {
  const { verificationCode: _, ...input } = request
  await respondToPairingMutate({ input, accepted: true })
}

export async function denyPairing(request: PairingRequest) {
  const { verificationCode: _, ...input } = request
  await respondToPairingMutate({ input, accepted: false })
}

export interface PairingDeviceInput { id: string; host?: string }

export function useDevicePairing() {
  const verificationCodes = ref<Map<string, string>>(new Map())
  async function refreshVerificationCodes() {
    const { data } = await gqlFetch<{ pairingStates: Pick<PairingState, 'deviceId' | 'phase' | 'verificationCode'>[] }>('{ pairingStates { deviceId phase verificationCode } }')
    const codes = new Map<string, string>()
    for (const state of data?.pairingStates ?? []) {
      if (state.deviceId && state.verificationCode && !['SUCCEEDED', 'FAILED', 'CANCELLED'].includes(state.phase)) {
        codes.set(state.deviceId, state.verificationCode)
      }
    }
    verificationCodes.value = codes
  }
  const refresh = () => { void refreshVerificationCodes().catch(error => console.error('Pairing verification refresh failed', error)) }
  let stopChanges: (() => void) | undefined
  let stopAttached: (() => void) | undefined
  onMounted(() => {
    stopChanges = graphqlSubscriptions.subscribe('pairingChanged', refresh)
    stopAttached = graphqlSubscriptions.onAttached(refresh)
    refresh()
  })
  onBeforeUnmount(() => { stopChanges?.(); stopAttached?.() })
  const generations = new Map<string, string>()
  const key = (id: string) => `${getGraphqlContextKey()}|${id}`
  async function pairDevice(device: PairingDeviceInput) {
    const contextKey = key(device.id)
    const result = await pairDeviceMutate({ input: device })
    if (result?.data?.pairDevice) generations.set(contextKey, result.data.pairDevice.generation)
    return result != null
  }

  async function cancelPairing(deviceId: string) {
    let generation = generations.get(key(deviceId))
    if (!generation) {
      const { data } = await import('@/lib/api/gql-client').then(({ gqlFetch }) => gqlFetch('{ pairingStates { deviceId generation phase } }'))
      generation = data?.pairingStates?.find((s: { deviceId: string; phase: string }) => s.deviceId === deviceId && !['SUCCEEDED', 'FAILED', 'CANCELLED'].includes(s.phase))?.generation
    }
    if (generation) await cancelPairingMutate({ generation })
  }

  return {
    pairDevice,
    cancelPairing,
    deviceStates,
    verificationCodes,
  }
}
