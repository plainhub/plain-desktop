import { expect, it, vi } from 'vitest'
import { effectScope, nextTick, ref } from 'vue'
const io = vi.hoisted(() => ({ fetch: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: io.fetch }))
import { useChatDraft } from '@/views/chat/hooks/chat-draft'

it('persists one input value once and clears it after repeated multiline input notifications', async () => {
  io.fetch.mockReset().mockImplementation(async (document: string) => ({ data: document.startsWith('query') ? { savedValue: '' } : { setSavedValue: true } }))
  const scope = effectScope()
  try {
    const draft = scope.run(() => useChatDraft(ref('peer:local')))!
    await nextTick()
    const text = '中文🦀\n'.repeat(750)
    for (let i = 0; i < 1500; i++) draft.value = text
    draft.value = ''
    const writes = () => io.fetch.mock.calls.filter(([document]) => document.startsWith('mutation'))
    await vi.waitFor(() => expect(writes().at(-1)?.[1].value).toBe(''))
    expect(writes().map(([, variables]) => variables)).toEqual([
      { key: 'chat_input_text:peer:local', value: text },
      { key: 'chat_input_text:peer:local', value: '' },
    ])
  } finally { scope.stop() }
})

it('keeps edits over delayed draft reads and writes to the destination captured at each edit', async () => {
  let finish!: (value: unknown) => void
  io.fetch.mockReset().mockImplementation(async (document: string) => ({ data: document.startsWith('query') ? { savedValue: 'New destination draft' } : { setSavedValue: true } }))
  io.fetch.mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const scope = effectScope()
  try {
    const id = ref('peer:first')
    const draft = scope.run(() => useChatDraft(id))!
    draft.value = 'First local edit'
    id.value = 'peer:second'
    await nextTick()
    await vi.waitFor(() => expect(draft.value).toBe('New destination draft'))
    draft.value = 'Second local edit'
    finish({ data: { savedValue: 'Stale first draft' } })
    await nextTick()
    expect(draft.value).toBe('Second local edit')
    await vi.waitFor(() => expect(io.fetch.mock.calls.filter(([document]) => document.startsWith('mutation')).map(([, variables]) => variables)).toEqual([
      { key: 'chat_input_text:peer:first', value: 'First local edit' },
      { key: 'chat_input_text:peer:second', value: 'Second local edit' },
    ]))
  } finally { finish?.({ data: { savedValue: '' } }); scope.stop() }
})
