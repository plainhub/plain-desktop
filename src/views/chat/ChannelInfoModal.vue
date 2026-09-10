<template>
  <v-modal @close="close">
    <template #headline>{{ $t('channel_info') }}</template>
    <template #content>
      <ul class="card list-items">
        <v-list-item :title="$t('channel_name')" :value="channel.name" />
      </ul>

      <div class="section-title">{{ $t('channel_members') }} ({{ channel.members.length }})</div>
      <ul class="card list-items">
        <ChannelMemberListItem v-for="member in enrichedMembers" :key="member.id" :member="member">
          <template v-if="isOwner && !member.isSelf && member.id !== channel.owner" #end>
            <v-outlined-button v-if="member.status === MemberStatus.PENDING" :loading="pendingIds.has(member.id)" :disabled="pendingIds.has(member.id)" @click.stop="cancelInvite(member.id)">{{ $t('cancel') }}</v-outlined-button>
            <v-outlined-button
v-else v-tooltip="$t('remove_member')" 
            :loading="pendingIds.has(member.id)" :disabled="pendingIds.has(member.id)" 
            @click.stop="removeMember(member.id)" >{{ $t('remove') }}</v-outlined-button>
          </template>
        </ChannelMemberListItem>
      </ul>

      <template v-if="isOwner && availablePeers.length > 0">
        <div class="section-title">{{ $t('add_member') }}</div>
        <ul class="card list-items">
          <ChannelMemberListItem
v-for="peer in availablePeers" :key="peer.id" :member="{ id: peer.id, name: peer.name, ip: peer.ip, deviceType: peer.deviceType, isSelf: false, isOwner: false, status: peer.status }">
            <template #end>
              <v-outlined-button :loading="pendingIds.has(peer.id)" :disabled="pendingIds.has(peer.id)" @click.stop="addMember(peer.id)">{{ $t('invite') }}</v-outlined-button>
            </template>
          </ChannelMemberListItem>
        </ul>
      </template>
    </template>
    <template #actions>
      <v-outlined-button @click="close">{{ $t('close') }}</v-outlined-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { ref, reactive, computed, onMounted, onUnmounted } from 'vue'
import type { PropType } from 'vue'
import type { IChatChannel, IPeer } from '@/lib/interfaces'
import { MemberStatus, PeerStatus } from '@/lib/status'
import { popModal } from '@/components/modal'
import { initMutation, addChatChannelMemberGQL, removeChatChannelMemberGQL } from '@/lib/api/mutation'
import { useTempStore } from '@/stores/temp'
import emitter from '@/plugins/eventbus'
import type { IChannelMemberListItem } from './components/ChannelMemberListItem.vue'

const props = defineProps({
  channel: { type: Object as PropType<IChatChannel>, required: true },
  peers: { type: Array as PropType<IPeer[]>, default: () => [] },
  onMemberUpdated: { type: Function as PropType<() => void>, default: () => { } },
})

const channel = ref({ ...props.channel })
const { app } = useTempStore()
const selfId = app.clientId
const deviceName = computed(() => app.deviceName || selfId.substring(0, 8))
const isOwner = computed(() => channel.value.owner === selfId)
const memberIds = computed(() => new Set(channel.value.members.map((m) => m.id)))
const availablePeers = computed(() => props.peers.filter((p) => p.status === PeerStatus.PAIRED && !memberIds.value.has(p.id)))

const enrichedMembers = computed<IChannelMemberListItem[]>(() =>
  channel.value.members.map((m) => {
    const peer = props.peers.find((p) => p.id === m.id)
    const isSelf = m.id === selfId
    const isOwner = m.id === channel.value.owner || (isSelf && channel.value.owner === selfId)
    return {
      id: m.id,
      status: m.status,
      isSelf,
      isOwner,
      name: isSelf ? deviceName.value : peer?.name ?? m.id.substring(0, 8),
      ip: isSelf ? undefined : peer?.ip,
      deviceType: isSelf ? app.deviceType : peer?.deviceType,
    }
  }),
)

const { mutate: mutateAddMember, onDone: onAddMemberDone } = initMutation({ document: addChatChannelMemberGQL })
const { mutate: mutateRemoveMember, onDone: onRemoveMemberDone } = initMutation({ document: removeChatChannelMemberGQL })

onAddMemberDone((r: any) => { if (r.data?.addChatChannelMember) channel.value = { ...r.data.addChatChannelMember }; props.onMemberUpdated() })
onRemoveMemberDone((r: any) => { if (r.data?.removeChatChannelMember) channel.value = { ...r.data.removeChatChannelMember }; props.onMemberUpdated() })

function onChannelsUpdated(list: any[]) {
  const updated = list?.find((c: any) => c.id === channel.value.id)
  if (updated) channel.value = { ...updated }
}

onMounted(() => { emitter.on('channels_updated', onChannelsUpdated) })
onUnmounted(() => { emitter.off('channels_updated', onChannelsUpdated) })

const pendingIds = reactive(new Set<string>())

function runMutation(id: string, mutate: (vars: any) => Promise<any>) {
  if (pendingIds.has(id)) return
  pendingIds.add(id)
  mutate({ id: channel.value.id, peerId: id }).finally(() => {
    pendingIds.delete(id)
  })
}

function addMember(peerId: string) { runMutation(peerId, mutateAddMember) }
function removeMember(peerId: string) { runMutation(peerId, mutateRemoveMember) }
function cancelInvite(peerId: string) { runMutation(peerId, mutateRemoveMember) }
function close() { popModal() }
</script>

