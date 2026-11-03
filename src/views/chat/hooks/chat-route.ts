import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { useMainStore } from '@/stores/main'
import { bitArrayToBase64, chachaDecrypt, chachaEncrypt } from '@/lib/api/crypto'
import { tokenToKey } from '@/lib/api/file'
import { replacePath } from '@/plugins/router'

export function encryptChatId(id: string, key: Uint8Array | null): string {
  return key ? bitArrayToBase64(chachaEncrypt(key, id)) : ''
}

export function decryptChatId(rawId: string, key: Uint8Array | null): string {
  if (!rawId || !key) return 'peer:local'
  try {
    const bits = tokenToKey(rawId)
    const decrypted = chachaDecrypt(key, bits)
    if (decrypted.startsWith('peer:') || decrypted.startsWith('channel:')) return decrypted
  } catch {
    // ignore malformed id
  }
  return 'peer:local'
}

export function useChatRouteId() {
  const route = useRoute()
  const store = useMainStore()
  const { urlTokenKey, app } = storeToRefs(useTempStore())

  const routeId = computed(() => {
    const qid = route.query.id
    return typeof qid === 'string' ? qid : ''
  })

  const chatId = computed(() => decryptChatId(routeId.value, urlTokenKey.value))
  const peerId = computed(() => (chatId.value.startsWith('peer:') ? chatId.value.slice(5) : ''))
  const channelId = computed(() => (chatId.value.startsWith('channel:') ? chatId.value.slice(8) : ''))
  const isChannel = computed(() => !!channelId.value)
  const appDir = app.value?.appDir ?? ''

  function openFolder() {
    replacePath(store, '/chat/app-files')
  }

  return { chatId, peerId, channelId, isChannel, routeId, appDir, openFolder }
}
