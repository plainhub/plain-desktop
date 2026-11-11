import { computed, ref, watch, type Ref } from 'vue'
import { readKeyValue, setKeyValue } from '@/lib/key-values'

export function useChatDraft(chatId: Ref<string>) {
  const draft = ref('')
  let draftVersion = 0
  let draftWrites = Promise.resolve()
  const draftKey = computed(() => `chat_input_text:${chatId.value}`)
  watch(draftKey, async (key) => {
    const version = ++draftVersion
    draft.value = ''
    try {
      const value = await readKeyValue(key, '')
      if (version === draftVersion) draft.value = value
    } catch (error) { console.error('Unable to load chat draft', error) }
  }, { immediate: true })
  return computed({
    get: () => draft.value,
    set: (value: string) => {
      if (value === draft.value) return
      ++draftVersion
      draft.value = value
      const key = draftKey.value
      draftWrites = draftWrites.then(() => setKeyValue(key, value)).catch(console.error)
    },
  })
}
