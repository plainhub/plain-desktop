/**
 * Group 2 — Mutation round-trip (common scenarios).
 *
 * Verifies that write operations return the same struct shape on
 * both servers. Each test creates a uniquely-named resource, asserts
 * the response, and cleans up in afterEach so repeated runs don't
 * accumulate state.
 *
 * plain-app (Android) is the source of truth for the expected shape.
 */
import { describe, it, expect, afterEach } from 'vitest'
import { chatChannelFragment, chatItemFragment } from '@/lib/api/fragments'
import {
  hasBothEndpoints,
  skipReason,
  gqlFetchBoth,
  gqlFetch,
  rustEndpoint,
  androidEndpoint,
  uniqueName,
} from './helpers'

// Track IDs created during tests so afterEach can delete them on
// both servers. Cleared after each test.
const rustCleanup: Array<{ type: 'channel' | 'chat'; id: string }> = []
const androidCleanup: Array<{ type: 'channel' | 'chat'; id: string }> = []

async function cleanupEndpoint(
  ep: typeof rustEndpoint,
  items: Array<{ type: 'channel' | 'chat'; id: string }>,
) {
  if (!ep) return
  for (const item of items) {
    const mutation =
      item.type === 'channel'
        ? `mutation($id: String!) { deleteChatChannel(id: $id) }`
        : `mutation($id: String!) { deleteChatItem(id: $id) }`
    try {
      await gqlFetch(ep, mutation, { id: item.id })
    } catch {
      // Best-effort cleanup — don't fail the test if cleanup errors.
    }
  }
}

describe.skipIf(!hasBothEndpoints)(`mutations (round-trip) — ${skipReason || 'both endpoints configured'}`, () => {
  afterEach(async () => {
    await Promise.all([
      cleanupEndpoint(rustEndpoint, rustCleanup.splice(0)),
      cleanupEndpoint(androidEndpoint, androidCleanup.splice(0)),
    ])
  })

  // 2.1 createChatChannel — returns the full ChatChannel struct.
  // Why: the chat creation flow renders the returned channel
  // immediately (optimistic UI). If the server omits a field the
  // frontend expects, the UI breaks. Both servers must return:
  //   - non-empty id
  //   - name matching the input
  //   - ownerId set to the local client id
  //   - members containing at least the owner (or empty —
  //     behaviorally the channel exists)
  //   - status (typically "active" or empty)
  //   - version = 1
  it('createChatChannel: both return ChatChannel with id, name, ownerId, members, version=1', async () => {
    const mutation = `
      mutation($name: String!) {
        createChatChannel(name: $name) {
          ...ChatChannelFragment
        }
      }
      ${chatChannelFragment}
    `
    const rustName = uniqueName('rust_ch')
    const androidName = uniqueName('android_ch')
    // Use separate gqlFetch calls (not gqlFetchBoth) because each
    // server needs a distinct name to avoid collisions.
    const rust = await gqlFetch<{ createChatChannel: any }>(rustEndpoint!, mutation, { name: rustName })
    const androidRes = await gqlFetch<{ createChatChannel: any }>(androidEndpoint!, mutation, { name: androidName })

    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(androidRes.errors, `android errors: ${JSON.stringify(androidRes.errors)}`).toBeUndefined()

    const rCh = rust.data!.createChatChannel
    const aCh = androidRes.data!.createChatChannel
    expect(rCh.id).toBeTruthy()
    expect(aCh.id).toBeTruthy()
    expect(rCh.name).toBe(rustName)
    expect(aCh.name).toBe(androidName)
    expect(typeof rCh.ownerId).toBe('string')
    expect(typeof aCh.ownerId).toBe('string')
    expect(Array.isArray(rCh.members)).toBe(true)
    expect(Array.isArray(aCh.members)).toBe(true)
    expect(rCh.version).toBe(1)
    expect(aCh.version).toBe(1)
    expect(typeof rCh.status).toBe('string')
    expect(typeof aCh.status).toBe('string')

    rustCleanup.push({ type: 'channel', id: rCh.id })
    androidCleanup.push({ type: 'channel', id: aCh.id })
  })

  // 2.2 updateChatChannel — name changes, version increments.
  // Why: rename is a common operation. The returned struct must
  // reflect the new name and have version = old + 1. If the server
  // doesn't bump version, clients miss the update.
  it('updateChatChannel: both return updated channel with version+1', async () => {
    const createMut = `
      mutation($name: String!) {
        createChatChannel(name: $name) {
          ...ChatChannelFragment
        }
      }
      ${chatChannelFragment}
    `
    const updateMut = `
      mutation($id: String!, $name: String!) {
        updateChatChannel(id: $id, name: $name) {
          ...ChatChannelFragment
        }
      }
      ${chatChannelFragment}
    `

    // Create on both servers.
    const rustName = uniqueName('rust_upd')
    const androidName = uniqueName('android_upd')
    const rustCreate = await gqlFetch(rustEndpoint!, createMut, { name: rustName })
    const androidCreate = await gqlFetch(androidEndpoint!, createMut, { name: androidName })
    const rustCh = rustCreate.data!.createChatChannel
    const androidCh = androidCreate.data!.createChatChannel
    rustCleanup.push({ type: 'channel', id: rustCh.id })
    androidCleanup.push({ type: 'channel', id: androidCh.id })

    // Update on both servers.
    const rustNewName = uniqueName('rust_renamed')
    const androidNewName = uniqueName('android_renamed')
    const rust = await gqlFetch<{ updateChatChannel: any }>(rustEndpoint!, updateMut, {
      id: rustCh.id,
      name: rustNewName,
    })
    const androidRes = await gqlFetch<{ updateChatChannel: any }>(androidEndpoint!, updateMut, {
      id: androidCh.id,
      name: androidNewName,
    })

    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(androidRes.errors, `android errors: ${JSON.stringify(androidRes.errors)}`).toBeUndefined()

    const rUpd = rust.data!.updateChatChannel
    const aUpd = androidRes.data!.updateChatChannel
    expect(rUpd.name).toBe(rustNewName)
    expect(aUpd.name).toBe(androidNewName)
    expect(rUpd.version).toBe(rustCh.version + 1)
    expect(aUpd.version).toBe(androidCh.version + 1)
  })

  // 2.3 deleteChatChannel — returns true for existing channel.
  // Why: deletion must be idempotent-ish and return a boolean
  // (not the deleted object). Both servers return true on success.
  it('deleteChatChannel: both return true for existing channel', async () => {
    const createMut = `
      mutation($name: String!) {
        createChatChannel(name: $name) { id }
      }
    `
    const deleteMut = `mutation($id: String!) { deleteChatChannel(id: $id) }`

    const rustCreate = await gqlFetch(rustEndpoint!, createMut, { name: uniqueName('rust_del') })
    const androidCreate = await gqlFetch(androidEndpoint!, createMut, { name: uniqueName('android_del') })
    const rustId = rustCreate.data!.createChatChannel.id
    const androidId = androidCreate.data!.createChatChannel.id

    const rust = await gqlFetch<{ deleteChatChannel: boolean }>(rustEndpoint!, deleteMut, { id: rustId })
    const androidRes = await gqlFetch<{ deleteChatChannel: boolean }>(androidEndpoint!, deleteMut, { id: androidId })

    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(androidRes.errors, `android errors: ${JSON.stringify(androidRes.errors)}`).toBeUndefined()
    expect(rust.data!.deleteChatChannel).toBe(true)
    expect(androidRes.data!.deleteChatChannel).toBe(true)
  })

  // 2.4 sendChatItem(toId="local") — returns [ChatItem] with status.
  // Why: local notes (toId="local") have no remote delivery, so
  // status should be set (not pending). The returned array must
  // have exactly 1 item whose content matches the input. This is
  // the minimal smoke test for the chat send path.
  it('sendChatItem(local): both return [ChatItem] with status set, content matches', async () => {
    const mutation = `
      mutation($toId: String!, $content: String!) {
        sendChatItem(toId: $toId, content: $content) {
          ...ChatItemFragment
        }
      }
      ${chatItemFragment}
    `
    // Minimal text content — both servers store it as-is.
    // The `data` field will be null (no images/files), which is valid.
    const content = JSON.stringify({
      type: 'text',
      value: { text: `integration-test ${Date.now()}` },
    })

    const { rust, android } = await gqlFetchBoth<{ sendChatItem: any[] }>(mutation, {
      toId: 'local',
      content,
    })

    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(android.errors, `android errors: ${JSON.stringify(android.errors)}`).toBeUndefined()

    expect(rust.data!.sendChatItem).toHaveLength(1)
    expect(android.data!.sendChatItem).toHaveLength(1)

    const rItem = rust.data!.sendChatItem[0]
    const aItem = android.data!.sendChatItem[0]
    expect(rItem.toId).toBe('local')
    expect(aItem.toId).toBe('local')
    expect(rItem.content).toBe(content)
    expect(aItem.content).toBe(content)
    expect(typeof rItem.status).toBe('string')
    expect(rItem.status.length).toBeGreaterThan(0)
    expect(typeof aItem.status).toBe('string')
    expect(aItem.status.length).toBeGreaterThan(0)
    expect(rItem.id).toBeTruthy()
    expect(aItem.id).toBeTruthy()

    rustCleanup.push({ type: 'chat', id: rItem.id })
    androidCleanup.push({ type: 'chat', id: aItem.id })
  })

  // 2.5 deleteChatItem — returns bool (true on success).
  // Why: chat deletion must return a boolean, not the deleted item.
  // Rust returns false when the id doesn't exist (handled in Group 3);
  // here we verify the happy path returns true.
  it('deleteChatItem: both return true for existing chat item', async () => {
    const sendMut = `
      mutation($toId: String!, $content: String!) {
        sendChatItem(toId: $toId, content: $content) { id }
      }
    `
    const deleteMut = `mutation($id: String!) { deleteChatItem(id: $id) }`

    const content = JSON.stringify({ type: 'text', value: { text: 'to-delete' } })
    const rustSend = await gqlFetch(rustEndpoint!, sendMut, { toId: 'local', content })
    const androidSend = await gqlFetch(androidEndpoint!, sendMut, { toId: 'local', content })
    const rustId = rustSend.data!.sendChatItem[0].id
    const androidId = androidSend.data!.sendChatItem[0].id

    const rust = await gqlFetch<{ deleteChatItem: boolean }>(rustEndpoint!, deleteMut, { id: rustId })
    const androidRes = await gqlFetch<{ deleteChatItem: boolean }>(androidEndpoint!, deleteMut, { id: androidId })

    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(androidRes.errors, `android errors: ${JSON.stringify(androidRes.errors)}`).toBeUndefined()
    expect(rust.data!.deleteChatItem).toBe(true)
    expect(androidRes.data!.deleteChatItem).toBe(true)
  })
})
