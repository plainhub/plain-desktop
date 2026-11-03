import { computed, ref, watch, type Ref } from 'vue'
import { readSavedValue, saveValue } from '@/lib/saved-values'

export function useChatDraft(chatId: Ref<string>) {
  const draft = ref('')
  let draftVersion = 0
  let draftWrites = Promise.resolve()
  const draftKey = computed(() => `chat_input_text:${chatId.value}`)
  watch(draftKey, async (key) => {
    const version = ++draftVersion
    draft.value = ''
    try {
      const value = await readSavedValue(key, '')
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
      draftWrites = draftWrites.then(() => saveValue(key, value)).catch(console.error)
    },
  })
}
