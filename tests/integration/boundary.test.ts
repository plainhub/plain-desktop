/**
 * Group 3 — Boundary tests (error handling consistency).
 *
 * Sends invalid / edge-case inputs to both servers and asserts they
 * behave consistently: if one errors, the other must error too; if
 * one succeeds, the other must succeed. Error *message text* is
 * allowed to differ — only the error/no-error behavior must match.
 *
 * plain-app (Android) is the source of truth.
 */
import { describe, it, expect } from 'vitest'
import {
  hasBothEndpoints,
  skipReason,
  gqlFetchBoth,
} from './helpers'

describe.skipIf(!hasBothEndpoints)(`boundary (error cases) — ${skipReason || 'both endpoints configured'}`, () => {
  // 3.1 updateChatChannel with non-existent id — both must error.
  // Why: a silent success on a missing resource is a bug — the
  // caller would believe the rename happened. Rust currently throws
  // "Channel not found"; Android's ChannelManager.renameChannel
  // must also surface an error. If either side returns data:null
  // with no errors, that's a divergence.
  it('updateChatChannel(nonexistent): both return GraphQL errors', async () => {
    const mutation = `
      mutation($id: String!, $name: String!) {
        updateChatChannel(id: $id, name: $name) { id }
      }
    `
    const { rust, android } = await gqlFetchBoth<{ updateChatChannel: any }>(mutation, {
      id: '__nonexistent_channel_id__',
      name: 'should-fail',
    })
    // Both must report errors — neither may silently return null data.
    expect(rust.errors, 'Rust should error on non-existent channel').toBeDefined()
    expect(rust.errors!.length).toBeGreaterThan(0)
    expect(android.errors, 'Android should error on non-existent channel').toBeDefined()
    expect(android.errors!.length).toBeGreaterThan(0)
  })

  // 3.2 deleteChatChannel with non-existent id — both return true.
  // Why: deletion of a missing resource is idempotent by design on
  // both servers (Rust returns true unconditionally; Android's
  // ChannelManager.deleteChannel is a no-op for missing ids). This
  // is the accepted contract — verify both sides honor it.
  it('deleteChatChannel(nonexistent): both return true (idempotent)', async () => {
    const mutation = `mutation($id: String!) { deleteChatChannel(id: $id) }`
    const { rust, android } = await gqlFetchBoth<{ deleteChatChannel: boolean }>(mutation, {
      id: '__nonexistent_channel_id__',
    })
    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(android.errors, `android errors: ${JSON.stringify(android.errors)}`).toBeUndefined()
    expect(rust.data!.deleteChatChannel).toBe(true)
    expect(android.data!.deleteChatChannel).toBe(true)
  })

  // 3.3 addChatChannelMember with non-existent channel — both error.
  // Why: inviting to a channel that doesn't exist must fail loudly.
  // Rust throws "Channel not found". Android must also reject it.
  // A silent success would leave the caller thinking the invite was
  // sent.
  it('addChatChannelMember(nonexistent channel): both return GraphQL errors', async () => {
    const mutation = `
      mutation($id: String!, $peerId: String!) {
        addChatChannelMember(id: $id, peerId: $peerId) { id }
      }
    `
    const { rust, android } = await gqlFetchBoth<{ addChatChannelMember: any }>(mutation, {
      id: '__nonexistent_channel_id__',
      peerId: '__nonexistent_peer_id__',
    })
    expect(rust.errors, 'Rust should error on non-existent channel').toBeDefined()
    expect(rust.errors!.length).toBeGreaterThan(0)
    expect(android.errors, 'Android should error on non-existent channel').toBeDefined()
    expect(android.errors!.length).toBeGreaterThan(0)
  })

  // 3.4 createChatChannel with empty name — both behave consistently.
  // Why: empty name is an edge case. Rust trims the name (line 27 of
  // chat_channel.rs: `name.trim()`) and creates a channel with "".
  // Android's ChannelManager.createChannel may reject or accept.
  // Whatever the behavior, both sides must agree — otherwise the web
  // client would create a channel on one server but fail on the
  // other. This test documents the current behavior; if they diverge,
  // it fails until aligned.
  it('createChatChannel(""): both behave consistently (both succeed or both error)', async () => {
    const mutation = `mutation($name: String!) { createChatChannel(name: $name) { id } }`
    const { rust, android } = await gqlFetchBoth<{ createChatChannel: any }>(mutation, {
      name: '',
    })
    // Either both succeed or both error — mismatch is the failure.
    const rustOk = !rust.errors
    const androidOk = !android.errors
    expect(rustOk).toBe(androidOk)

    // If both succeeded, clean up (best-effort — the channel exists
    // on both servers with an empty name; we leave it since we don't
    // have the id here without another query. The empty-name channel
    // is harmless and will be visible in the UI for manual cleanup.)
  })

  // 3.5 sendChatItem with empty content — both behave consistently.
  // Why: empty content is malformed (no JSON type/value). Both
  // servers currently accept it (store the empty string). The
  // contract is: whatever happens, both sides agree. If one starts
  // rejecting empty content in the future, this test catches the
  // divergence.
  it('sendChatItem(local, ""): both behave consistently', async () => {
    const mutation = `
      mutation($toId: String!, $content: String!) {
        sendChatItem(toId: $toId, content: $content) { id }
      }
    `
    const { rust, android } = await gqlFetchBoth<{ sendChatItem: any[] }>(mutation, {
      toId: 'local',
      content: '',
    })
    const rustOk = !rust.errors
    const androidOk = !android.errors
    expect(rustOk).toBe(androidOk)
  })

  // 3.6 chatItems with empty id — graceful handling.
  // Why: an empty id is not a valid ChatTarget, but the server must
  // not crash. Both should return an empty array (or a consistent
  // error). The chat list could accidentally pass "" before the user
  // selects a conversation.
  it('chatItems(""): both handle empty id gracefully (empty array or consistent error)', async () => {
    const query = `
      query($id: String!) {
        chatItems(id: $id, offset: 0, limit: 200) { id }
      }
    `
    const { rust, android } = await gqlFetchBoth<{ chatItems: any[] }>(query, { id: '' })
    const rustOk = !rust.errors
    const androidOk = !android.errors
    // Both must agree on error vs success.
    expect(rustOk).toBe(androidOk)
    // If both succeeded, both must return an array (not null).
    if (rustOk) {
      expect(Array.isArray(rust.data?.chatItems)).toBe(true)
      expect(Array.isArray(android.data?.chatItems)).toBe(true)
    }
  })
})
