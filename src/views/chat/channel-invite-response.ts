import { ref } from 'vue'
import { acceptChatChannelInviteGQL, declineChatChannelInviteGQL, initMutation } from '@/lib/api/mutation'

export function useChannelInviteResponse(
  channelId: string,
  onResponded: (id: string, accepted: boolean) => void,
  close: () => void,
) {
  const loading = ref(false)
  const accept = initMutation({ document: acceptChatChannelInviteGQL })
  const decline = initMutation({ document: declineChatChannelInviteGQL })
  accept.onDone(() => { onResponded(channelId, true); close() })
  decline.onDone(() => { onResponded(channelId, false); close() })
  accept.onError(close)
  decline.onError(close)

  async function respond(accepted: boolean) {
    if (loading.value) return
    loading.value = true
    try {
      await (accepted ? accept : decline).mutate({ id: channelId })
    } finally {
      loading.value = false
    }
  }
  return { loading, respond }
}
