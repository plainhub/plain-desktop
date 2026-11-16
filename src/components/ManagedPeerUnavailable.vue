<template>
  <v-modal @close="popModal">
    <template #headline>
      {{ peerName ? $t('managed_peer_unavailable_title', { name: peerName }) : $t('managed_peer_unavailable') }}
    </template>
    <template #content>
      <p>{{ $t('managed_peer_pairing_hint') }}</p>
    </template>
    <template #actions>
      <v-text-button @click="switchDevice('')">
        {{ originName ? $t('managed_peer_switch_origin', { name: originName }) : $t('managed_peer_switch_local') }}
      </v-text-button>
      <v-filled-button @click="pairAgain">{{ $t('managed_peer_pair_again') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { popModal, pushModal } from '@/components/modal'
import { switchDevice } from '@/lib/device/switch-device'
import NearbyView from '@/views/chat/NearbyView.vue'

defineProps<{ peerName?: string; originName?: string }>()

async function pairAgain() {
  await popModal()
  await pushModal(NearbyView, { origin: true })
}
</script>
