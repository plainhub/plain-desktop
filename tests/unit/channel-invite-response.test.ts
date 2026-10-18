import { beforeEach, describe, expect, it, vi } from 'vitest'

const transport = vi.hoisted(() => ({ operations: [] as Array<any> }))
vi.mock('@/lib/api/mutation', () => ({
  acceptChatChannelInviteGQL: 'accept-invite',
  declineChatChannelInviteGQL: 'decline-invite',
  initMutation: ({ document }: { document: string }) => {
    const operation = { document, done: [] as Array<(result: any) => void>, errors: [] as Array<() => void>, mutate: vi.fn() }
    operation.mutate.mockImplementation(async () => {
      operation.done.forEach((callback) => callback({ data: { result: true } }))
    })
    transport.operations.push(operation)
    return { mutate: operation.mutate, onDone: (callback: any) => operation.done.push(callback), onError: (callback: any) => operation.errors.push(callback) }
  },
}))

import { useChannelInviteResponse } from '@/views/chat/channel-invite-response'

beforeEach(() => { transport.operations.length = 0 })

describe('channel invitation response', () => {
  it.each([true, false])('sends one existing operation and reports the clicked choice %s', async (accepted) => {
    const responded = vi.fn()
    const close = vi.fn()
    const state = useChannelInviteResponse('channel', responded, close)
    expect(transport.operations.every((operation) => operation.mutate.mock.calls.length === 0)).toBe(true)
    await state.respond(accepted)
    const selected = transport.operations.find((operation) => operation.document === (accepted ? 'accept-invite' : 'decline-invite'))!
    expect(selected.mutate).toHaveBeenCalledExactlyOnceWith({ id: 'channel' })
    expect(transport.operations.find((operation) => operation !== selected)!.mutate).not.toHaveBeenCalled()
    expect(responded).toHaveBeenCalledExactlyOnceWith('channel', accepted)
    expect(close).toHaveBeenCalledTimes(1)
    expect(state.loading.value).toBe(false)
  })

  it('ignores another click while the first operation is pending', async () => {
    const responded = vi.fn()
    const close = vi.fn()
    const state = useChannelInviteResponse('channel', responded, close)
    let finish!: () => void
    transport.operations[1].mutate.mockImplementation(() => new Promise<void>((resolve) => { finish = resolve }))
    const pending = state.respond(false)
    await state.respond(true)
    expect(transport.operations[1].mutate).toHaveBeenCalledTimes(1)
    expect(transport.operations[0].mutate).not.toHaveBeenCalled()
    transport.operations[1].done.forEach((callback: any) => callback({ data: { declineChatChannelInvite: true } }))
    finish()
    await pending
    expect(responded).toHaveBeenCalledExactlyOnceWith('channel', false)
    expect(close).toHaveBeenCalledTimes(1)
  })

  it('closes on an operation error without reporting acceptance', async () => {
    const responded = vi.fn()
    const close = vi.fn()
    const state = useChannelInviteResponse('channel', responded, close)
    transport.operations[0].mutate.mockImplementation(async () => {
      transport.operations[0].errors.forEach((callback: any) => callback())
    })
    await state.respond(true)
    expect(close).toHaveBeenCalledTimes(1)
    expect(responded).not.toHaveBeenCalled()
    expect(state.loading.value).toBe(false)
  })
})
