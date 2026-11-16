import { computed, ref, watch, type Ref } from 'vue'
import { readKeyValue, setKeyValue } from '@/lib/key-values'

export function useChatDraft(chatId: Ref<string>) {
  const draft = ref('')
  let draftVersion = 0
  let draftWrites: Promise<void> | undefined
  const edits = new Map<string, number>()
  const draftKey = computed(() => `chat_input_text:${chatId.value}`)
  const versionOf = (key: string) => edits.get(key) ?? 0

  watch(draftKey, async (key) => {
    const version = ++draftVersion
    draft.value = ''
    try {
      if (draftWrites) await draftWrites
      if (version !== draftVersion) return
      const value = await readKeyValue(key, '')
      if (version === draftVersion) draft.value = value
    } catch (error) { console.error('Unable to load chat draft', error) }
  }, { immediate: true })

  const value = computed({
    get: () => draft.value,
    set: (text: string) => {
      if (text === draft.value) return
      ++draftVersion
      draft.value = text
      const key = draftKey.value
      edits.set(key, versionOf(key) + 1)
      draftWrites = (draftWrites ?? Promise.resolve()).then(() => setKeyValue(key, text)).catch(console.error)
    },
  })

  function snapshot() {
    return { key: draftKey.value, text: draft.value, version: versionOf(draftKey.value) }
  }

  function clearSent({ key, text, version }: ReturnType<typeof snapshot>) {
    if (version !== versionOf(key)) return
    if (key === draftKey.value) {
      if (value.value === text) value.value = ''
      return
    }
    draftWrites = (draftWrites ?? Promise.resolve()).then(async () => {
      if (version !== versionOf(key) || await readKeyValue(key, '') !== text || version !== versionOf(key)) return
      if (key === draftKey.value && draft.value && draft.value !== text) return
      edits.set(key, version + 1)
      if (key === draftKey.value) {
        ++draftVersion
        draft.value = ''
      }
      await setKeyValue(key, '')
    }).catch(console.error)
  }

  return Object.assign(value, { snapshot, clearSent })
}
