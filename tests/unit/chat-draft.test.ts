import { expect, it, vi } from 'vitest'
import { effectScope, nextTick, ref } from 'vue'
const io = vi.hoisted(() => ({ fetch: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch: io.fetch }))
import { useChatDraft } from '@/views/chat/hooks/chat-draft'

it('persists one input value once and clears it after repeated multiline input notifications', async () => {
  io.fetch.mockReset().mockImplementation(async (document: string) => ({ data: document.startsWith('query') ? { keyValue: '' } : { setKeyValue: true } }))
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
  io.fetch.mockReset().mockImplementation(async (document: string) => ({ data: document.startsWith('query') ? { keyValue: 'New destination draft' } : { setKeyValue: true } }))
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
    finish({ data: { keyValue: 'Stale first draft' } })
    await nextTick()
    expect(draft.value).toBe('Second local edit')
    await vi.waitFor(() => expect(io.fetch.mock.calls.filter(([document]) => document.startsWith('mutation')).map(([, variables]) => variables)).toEqual([
      { key: 'chat_input_text:peer:first', value: 'First local edit' },
      { key: 'chat_input_text:peer:second', value: 'Second local edit' },
    ]))
  } finally { finish?.({ data: { keyValue: '' } }); scope.stop() }
})

it('clears the sent chat after switching destinations without disturbing the visible draft', async () => {
  const saved = new Map([['chat_input_text:peer:second', 'Keep second']])
  io.fetch.mockReset().mockImplementation(async (document: string, { key, value }) => {
    if (document.startsWith('query')) return { data: { keyValue: saved.get(key) ?? '' } }
    saved.set(key, value)
    return { data: { setKeyValue: true } }
  })
  const scope = effectScope()
  try {
    const id = ref('peer:first')
    const draft = scope.run(() => useChatDraft(id))!
    draft.value = 'First submitted'
    const submitted = draft.snapshot()
    id.value = 'peer:second'
    await vi.waitFor(() => expect(draft.value).toBe('Keep second'))
    draft.clearSent(submitted)
    await vi.waitFor(() => expect(saved.get('chat_input_text:peer:first')).toBe(''))
    expect(draft.value).toBe('Keep second')
    id.value = 'peer:first'
    await nextTick()
    expect(draft.value).toBe('')
  } finally { scope.stop() }
})

it('does not clear a newer edit in the sent chat even after navigating away', async () => {
  const saved = new Map<string, string>()
  io.fetch.mockReset().mockImplementation(async (document: string, { key, value }) => {
    if (document.startsWith('query')) return { data: { keyValue: saved.get(key) ?? '' } }
    saved.set(key, value)
    return { data: { setKeyValue: true } }
  })
  const scope = effectScope()
  try {
    const id = ref('peer:first')
    const draft = scope.run(() => useChatDraft(id))!
    draft.value = 'Same text'
    const sent = draft.snapshot()
    draft.value = 'New version'
    draft.value = 'Same text'
    id.value = 'peer:second'
    await nextTick()
    draft.clearSent(sent)
    await vi.waitFor(() => expect(io.fetch.mock.calls.filter(([document]) => document.startsWith('mutation'))).toHaveLength(3))
    expect(saved.get('chat_input_text:peer:first')).toBe('Same text')
  } finally { scope.stop() }
})
