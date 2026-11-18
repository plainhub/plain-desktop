<template>
  <v-modal :modal-id="modalId" @close="close">
    <template #headline>{{ $t('forward_message') }}</template>
    <template #content>
      <div :inert="submitting">
        <ul class="list-items forward-targets">
          <v-list-item
            v-for="target in targets"
            :key="target.id"
            :title="target.name"
            :subtitle="outcomes.has(target.id) ? $t(outcomes.get(target.id)!) : undefined"
            :clickable="!outcomes.has(target.id)"
            @click="toggle(target.id)"
          >
            <template #start>
              <i-lucide:hash v-if="target.channel" />
              <DeviceTypeIcon v-else :device-type="target.deviceType" />
            </template>
            <template #end>
              <span :inert="outcomes.has(target.id)">
                <v-checkbox
                  :label="target.name"
                  :checked="selected.has(target.id) || outcomes.has(target.id)"
                  @click.stop
                  @update:model-value="toggle(target.id)"
                />
              </span>
            </template>
          </v-list-item>
        </ul>
        <v-text-field v-model="note" type="textarea" variant="filled" :rows="2" :placeholder="$t('forward_note_optional')" />
      </div>
      <p v-if="hasUnconfirmed" role="alert">{{ $t('forward_unconfirmed_hint') }}</p>
      <p v-else-if="hasDeliveryFailure" role="alert">{{ $t('forward_failed_hint') }}</p>
    </template>
    <template #actions>
      <v-text-button :disabled="submitting" @click="close">{{ $t('cancel') }}</v-text-button>
      <v-filled-button :loading="submitting" :disabled="submitting || !selectedTargets.length" @click="doForward">
        {{ selectedTargets.length ? $t('forward_send_count', { count: selectedTargets.length }) : $t('send') }}
      </v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue'
import { closeById, getCurrentModal, modalQueue } from '@/components/modal'
import DeviceTypeIcon from '@/components/DeviceTypeIcon.vue'
import { bindGraphqlClient } from '@/lib/api/gql-client'
import { forwardChatItemsGQL } from '@/lib/api/mutation'
import { useChatStore } from '@/stores/chat'
import type { IChatItem } from '@/lib/interfaces'
import { ChatStatus } from '@/lib/status'

const props = withDefaults(defineProps<{ modalId: string; message: IChatItem; excludeChatId?: string }>(), { excludeChatId: '' })
const id = Number(props.modalId.replace(/\D/g, ''))
const chatStore = useChatStore()
const selected = reactive(new Set<string>())
const outcomes = reactive(new Map<string, 'forward_submitted' | 'forward_delivery_failed' | 'forward_unconfirmed'>())
const note = ref('')
const submitting = ref(false)

function preventEscape(event: KeyboardEvent) {
  if (submitting.value && (event.key === 'Escape' || event.code === 'Escape') && getCurrentModal()?.id === id) {
    event.stopImmediatePropagation()
  }
}

// The modal container also closes the top dialog on Escape keyup.
onMounted(() => document.addEventListener('keyup', preventEscape, true))
onUnmounted(() => document.removeEventListener('keyup', preventEscape, true))

const targets = computed(() => [
  ...chatStore.joinedChannels.map(channel => ({ id: `channel:${channel.id}`, name: channel.name, channel: true, deviceType: undefined })),
  ...chatStore.pairedPeers.map(peer => ({ id: `peer:${peer.id}`, name: peer.name, channel: false, deviceType: peer.deviceType })),
].filter(target => target.id !== props.excludeChatId))

const selectedTargets = computed(() => targets.value.filter(target => selected.has(target.id)))
const hasUnconfirmed = computed(() => [...outcomes.values()].includes('forward_unconfirmed'))
const hasDeliveryFailure = computed(() => [...outcomes.values()].includes('forward_delivery_failed'))

function toggle(id: string) {
  if (submitting.value || outcomes.has(id)) return
  if (!selected.delete(id)) selected.add(id)
}

async function doForward() {
  if (submitting.value || !selectedTargets.value.length) return
  submitting.value = true
  const recipients = selectedTargets.value.map(target => target.id)
  const draft = note.value.trim()
  try {
    let rows: IChatItem[] = []
    try {
      const request = bindGraphqlClient()
      const result = await request<{ forwardChatItems: IChatItem[] }>(forwardChatItemsGQL, {
        id: props.message.id, targets: recipients, ...(draft ? { note: draft } : {}),
      })
      rows = result.data?.forwardChatItems ?? []
    } catch {
      // Transport failure may have followed a committed mutation.
    }
    for (const id of recipients) {
      selected.delete(id)
      const messages = rows.filter(row => (row.channelId ? `channel:${row.channelId}` : `peer:${row.toId}`) === id)
      if (messages.length !== (draft ? 2 : 1)) {
        outcomes.set(id, 'forward_unconfirmed')
      } else {
        outcomes.set(id, messages.some(row => row.status === ChatStatus.FAILED || row.status === ChatStatus.PARTIAL)
          ? 'forward_delivery_failed' : 'forward_submitted')
      }
    }
    if (!hasUnconfirmed.value && !hasDeliveryFailure.value) await dismiss()
  } finally {
    submitting.value = false
  }
}

function dismiss() {
  if (modalQueue.value.some(modal => modal.id === id)) return closeById(id)
}

function close() {
  if (!submitting.value) void dismiss()
}
</script>

<style scoped>
.forward-targets {
  max-block-size: min(320px, 40vh);
  overflow-y: auto;
}
</style>
