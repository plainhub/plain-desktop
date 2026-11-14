import {
  cancelPairingGQL,
  initMutation,
  pairPeerGQL,
  respondToPairingGQL,
} from '@/lib/api/mutation'
import { pairingViews } from '@/lib/device/pairing'
import type { PairingRequest } from '@/lib/pairing-types'
import { computed, reactive } from 'vue'

export type { PairingRequest }
export enum PairingStatus {
  IDLE,
  REQUESTING,
  WAITING,
  SUCCESS,
  FAILED,
  CANCELED,
}

export enum DeviceState {
  UNPAIRED,
  PAIRING,
  PAIRED,
  UNPAIRING,
  CANCELING,
}
const deviceStates = reactive(new Map<string, DeviceState>())
const { mutate: pairPeerMutate } = initMutation({ document: pairPeerGQL }, false)
const { mutate: cancelPairingMutate } = initMutation({ document: cancelPairingGQL }, false)
const { mutate: respondToPairingMutate } = initMutation({ document: respondToPairingGQL })

export async function allowPairing(request: PairingRequest) {
  await respondToPairingMutate({ peerId: request.fromId, accepted: true })
}

export async function denyPairing(request: PairingRequest) {
  await respondToPairingMutate({ peerId: request.fromId, accepted: false })
}

export interface PairPeerInput { peerId: string; mode: 'CHAT' | 'MANAGE'; password?: string }

export function useDevicePairing() {
  const verificationCodes = computed(() => new Map(Array.from(pairingViews.values())
    .filter(view => view.verificationCode && !['SUCCEEDED', 'FAILED', 'CANCELED'].includes(view.phase))
    .map(view => [view.deviceId, view.verificationCode!])))
  async function pairPeer(device: PairPeerInput) {
    return await pairPeerMutate({ input: device }) != null
  }

  async function cancelPairing(peerId: string) {
    await cancelPairingMutate({ peerId })
  }

  return {
    pairPeer,
    cancelPairing,
    deviceStates,
    verificationCodes,
  }
}
