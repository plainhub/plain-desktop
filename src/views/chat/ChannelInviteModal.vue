<template>
  <v-modal @close="dismiss">
    <template #headline>{{ $t('channel_invite') }}</template>
    <template #content>
      <p class="desc">
        {{ $t('channel_invite_desc', { name: invite.channelName }) }}
      </p>
      <p v-if="invite.fromName" class="from">
        {{ $t('from') }}: <strong>{{ invite.fromName }}</strong>
      </p>
    </template>
    <template #actions>
      <v-outlined-button :loading="loading" :disabled="loading" @click="respond(false)">
        {{ $t('decline_invite') }}
      </v-outlined-button>
      <v-filled-button :loading="loading" :disabled="loading" @click="respond(true)">
        {{ $t('accept_invite') }}
      </v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { popModal } from '@/components/modal'
import { useChannelInviteResponse } from './channel-invite-response'

interface IChannelInvite {
  channelId: string
  channelName: string
  fromId: string
  fromName: string
}

const props = defineProps<{
  invite: IChannelInvite
  onResponded?: (channelId: string, accepted: boolean) => void
}>()

const { loading, respond } = useChannelInviteResponse(
  props.invite.channelId,
  (id, accepted) => props.onResponded?.(id, accepted),
  popModal,
)

function dismiss() {
  popModal()
}
</script>

<style scoped>
.desc {
  margin: 0 0 8px 0;
  line-height: 1.4;
}
.from {
  margin: 0;
  font-size: 13px;
  opacity: 0.75;
}
</style>
