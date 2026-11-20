import { showActionError } from '@/hooks/show-action-error'
import { initMutation, respondToPairingGQL } from '@/lib/api/mutation'
import type { PairingRequest } from '@/lib/pairing-types'

const { mutate: respondToPairingMutate } = initMutation({ document: respondToPairingGQL }, showActionError)

export async function allowPairing(request: PairingRequest) {
  await respondToPairingMutate({ peerId: request.fromId, accepted: true })
}

export async function denyPairing(request: PairingRequest) {
  await respondToPairingMutate({ peerId: request.fromId, accepted: false })
}
