import {
  cancelPairingGQL,
  initMutation,
  pairDeviceGQL,
  respondToPairingGQL,
} from '@/lib/api/mutation'
import type { PairingRequest } from '@/lib/pairing-types'
import { getGraphqlContextKey } from '@/lib/api/gql-client'
import { reactive } from 'vue'

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
const { mutate: respondToPairingMutate } = initMutation({ document: respondToPairingGQL }, false)

export async function allowPairing(request: PairingRequest) {
  await respondToPairingMutate({ input: request, accepted: true })
}

export async function denyPairing(request: PairingRequest) {
  await respondToPairingMutate({ input: request, accepted: false })
}

export interface PairingDeviceInput { id: string; host?: string }

export function useDevicePairing() {
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
  }
}
