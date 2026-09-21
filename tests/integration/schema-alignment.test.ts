/**
 * Group 1 — Schema alignment (read-only queries).
 *
 * Sends the same GraphQL read operations to both the Rust local
 * server and the Android device server, then asserts the returned
 * data has the same shape (field set + value types) on both sides.
 * plain-app (Android) is the source of truth.
 *
 * These tests do not mutate state — safe to run repeatedly.
 */
import { describe, it, expect } from 'vitest'
import {
  chatItemFragment,
  chatChannelFragment,
} from '@/lib/api/fragments'
import {
  hasBothEndpoints,
  skipReason,
  gqlFetchBoth,
} from './helpers'

// Skip the whole file when either endpoint isn't configured.
// Vitest reports skipped files in the output so the user knows
// they exist even when not running.
describe.skipIf(!hasBothEndpoints)(`schema alignment (read queries) — ${skipReason || 'both endpoints configured'}`, () => {
  // 1.1 peers — field set must match. Android's Peer model includes
  // `online` (from PeerStatusManager.isOnline); Rust's Peer struct
  // currently omits it. This test will fail on Rust until the field
  // is added.
  //
  // Why: the frontend's peersGQL (src/lib/api/query.ts) queries
  // `online`. If Rust lacks the field, the web local-mode chat list
  // can't render peer online status.
  it('peers: both return arrays with identical Peer field set (incl. online)', async () => {
    const query = `
      query {
        peers {
          id
          name
          ip
          status
          online
          port
          deviceType
          createdAt
          updatedAt
        }
      }
    `
    const { rust, android } = await gqlFetchBoth<{ peers: any[] }>(query)

    // Both must succeed without GraphQL errors.
    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(android.errors, `android errors: ${JSON.stringify(android.errors)}`).toBeUndefined()

    // Both must return arrays.
    expect(Array.isArray(rust.data?.peers)).toBe(true)
    expect(Array.isArray(android.data?.peers)).toBe(true)

    // If either side has data, compare the first element's shape.
    // Empty arrays on both sides also pass (different DB states).
    if (rust.data!.peers.length > 0 || android.data!.peers.length > 0) {
      const rustPeer = rust.data!.peers[0] ?? android.data!.peers[0]
      const androidPeer = android.data!.peers[0] ?? rust.data!.peers[0]
      // Explicitly list the expected field set (Android = source of truth).
      const expectedKeys = [
        'id', 'name', 'ip', 'status', 'online',
        'port', 'deviceType', 'createdAt', 'updatedAt',
      ].sort()
      expect(Object.keys(rustPeer).sort()).toEqual(expectedKeys)
      expect(Object.keys(androidPeer).sort()).toEqual(expectedKeys)
      // Types must match.
      for (const key of expectedKeys) {
        expect(typeof rustPeer[key]).toBe(typeof androidPeer[key])
      }
    }
  })

  // 1.2 chatChannels — members must be a nested array of {id, status}.
  // Why: the chat list view depends on members being structured, not
  // a JSON string. Rust stores members as a JSON string in DChannel
  // and parses it in ChatChannel::from; this test catches any parsing
  // regression that would flatten or omit the nested structure.
  it('chatChannels: both return arrays with nested members{id,status}', async () => {
    const query = `
      query {
        chatChannels {
          ...ChatChannelFragment
        }
      }
      ${chatChannelFragment}
    `
    const { rust, android } = await gqlFetchBoth<{ chatChannels: any[] }>(query)
    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(android.errors, `android errors: ${JSON.stringify(android.errors)}`).toBeUndefined()
    expect(Array.isArray(rust.data?.chatChannels)).toBe(true)
    expect(Array.isArray(android.data?.chatChannels)).toBe(true)

    if (rust.data!.chatChannels.length > 0 || android.data!.chatChannels.length > 0) {
      const r = rust.data!.chatChannels[0] ?? android.data!.chatChannels[0]
      const a = android.data!.chatChannels[0] ?? rust.data!.chatChannels[0]
      const expectedKeys = [
        'id', 'name', 'owner', 'members', 'version',
        'status', 'createdAt', 'updatedAt',
      ].sort()
      expect(Object.keys(r).sort()).toEqual(expectedKeys)
      expect(Object.keys(a).sort()).toEqual(expectedKeys)
      // members must be an array of {id, status}
      expect(Array.isArray(r.members)).toBe(true)
      expect(Array.isArray(a.members)).toBe(true)
      if (r.members.length > 0) {
        expect(Object.keys(r.members[0]).sort()).toEqual(['id', 'status'])
      }
      if (a.members.length > 0) {
        expect(Object.keys(a.members[0]).sort()).toEqual(['id', 'status'])
      }
      // version is i64 in Rust, Int in Android — both serialize to number.
      expect(typeof r.version).toBe('number')
      expect(typeof a.version).toBe('number')
    }
  })

  // 1.3 chatItems with non-existent peer id — graceful empty array.
  // Why: an invalid target id must not crash either server or return
  // a GraphQL error. Both should return [] (or null on one side, but
  // never throw). This is the contract the chat list relies on when
  // rendering a conversation that has no messages yet.
  it('chatItems(peer:nonexistent): both return empty array, no errors', async () => {
    const query = `
      query ($id: String!) {
        chatItems(id: $id, offset: 0, limit: 200) {
          ...ChatItemFragment
        }
      }
      ${chatItemFragment}
    `
    const { rust, android } = await gqlFetchBoth<{ chatItems: any[] }>(query, {
      id: 'peer:__nonexistent_peer_id__',
    })
    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(android.errors, `android errors: ${JSON.stringify(android.errors)}`).toBeUndefined()
    expect(rust.data?.chatItems).toEqual([])
    expect(android.data?.chatItems).toEqual([])
  })

  // 1.4 chatItems(id="local") — ChatItem shape with the data union.
  // Why: `local` is a synthetic target used for self-notes. Even if
  // no chats exist, the query must succeed. When chats do exist, the
  // `data` field is a polymorphic union (MessageImages / MessageFiles
  // / MessageText) — both servers must resolve the `... on X { ids }`
  // fragment selections without error.
  it('chatItems(local): ChatItem shape incl. data union resolves on both', async () => {
    const query = `
      query ($id: String!) {
        chatItems(id: $id, offset: 0, limit: 200) {
          ...ChatItemFragment
        }
      }
      ${chatItemFragment}
    `
    const { rust, android } = await gqlFetchBoth<{ chatItems: any[] }>(query, {
      id: 'local',
    })
    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(android.errors, `android errors: ${JSON.stringify(android.errors)}`).toBeUndefined()
    expect(Array.isArray(rust.data?.chatItems)).toBe(true)
    expect(Array.isArray(android.data?.chatItems)).toBe(true)

    // If both sides have data, compare the full shape including the
    // union `data` field. `data` may be null on either side (depends
    // on chat content) — that's allowed.
    if (rust.data!.chatItems.length > 0 && android.data!.chatItems.length > 0) {
      const expectedKeys = [
        'id', 'fromId', 'toId', 'channelId', 'createdAt',
        'updatedAt', 'content', 'status', 'statusData', 'data',
      ].sort()
      expect(Object.keys(rust.data!.chatItems[0]).sort()).toEqual(expectedKeys)
      expect(Object.keys(android.data!.chatItems[0]).sort()).toEqual(expectedKeys)
    }
  })

  // 1.5 latestChatItems — array of ChatItem (same shape as 1.4).
  // Why: the chat list view's "latest message per conversation"
  // rendering depends on this returning an array (never null), even
  // when empty.
  it('latestChatItems: both return arrays of ChatItem', async () => {
    const query = `
      query {
        latestChatItems {
          ...ChatItemFragment
        }
      }
      ${chatItemFragment}
    `
    const { rust, android } = await gqlFetchBoth<{ latestChatItems: any[] }>(query)
    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(android.errors, `android errors: ${JSON.stringify(android.errors)}`).toBeUndefined()
    expect(Array.isArray(rust.data?.latestChatItems)).toBe(true)
    expect(Array.isArray(android.data?.latestChatItems)).toBe(true)
  })

  // 1.6 appFiles(offset:0, limit:1) + appFileCount — paginated file list.
  // Why: the files manager uses pagination. `limit:1` is the minimal
  // non-empty page — verifies the server respects the limit and the
  // returned AppFile struct has the expected field set. appFileCount
  // must be a number on both sides.
  it('appFiles(0,1) + appFileCount: both return ≤1 file with correct fields + count', async () => {
    const query = `
      query appFiles($offset: Int!, $limit: Int!) {
        appFiles(offset: $offset, limit: $limit) {
          id
          size
          mimeType
          fileName
          createdAt
          updatedAt
        }
        appFileCount
      }
    `
    const { rust, android } = await gqlFetchBoth<{ appFiles: any[]; appFileCount: number }>(query, {
      offset: 0,
      limit: 1,
    })
    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(android.errors, `android errors: ${JSON.stringify(android.errors)}`).toBeUndefined()
    expect(Array.isArray(rust.data?.appFiles)).toBe(true)
    expect(Array.isArray(android.data?.appFiles)).toBe(true)
    expect(rust.data!.appFiles.length).toBeLessThanOrEqual(1)
    expect(android.data!.appFiles.length).toBeLessThanOrEqual(1)
    expect(typeof rust.data!.appFileCount).toBe('number')
    expect(typeof android.data!.appFileCount).toBe('number')

    // If either side returned a file, verify its field set.
    const rFile = rust.data!.appFiles[0]
    const aFile = android.data!.appFiles[0]
    const expectedKeys = ['id', 'size', 'mimeType', 'fileName', 'createdAt', 'updatedAt'].sort()
    if (rFile) expect(Object.keys(rFile).sort()).toEqual(expectedKeys)
    if (aFile) expect(Object.keys(aFile).sort()).toEqual(expectedKeys)
  })

  // 1.7 appFiles(offset:999999) — offset beyond range returns empty.
  // Why: boundary case. A paginated UI scrolls past the end; the
  // server must return [] (not an error, not null).
  it('appFiles(999999,10): both return empty array for out-of-range offset', async () => {
    const query = `
      query appFiles($offset: Int!, $limit: Int!) {
        appFiles(offset: $offset, limit: $limit) {
          id
        }
      }
    `
    const { rust, android } = await gqlFetchBoth<{ appFiles: any[] }>(query, {
      offset: 999999,
      limit: 10,
    })
    expect(rust.errors, `rust errors: ${JSON.stringify(rust.errors)}`).toBeUndefined()
    expect(android.errors, `android errors: ${JSON.stringify(android.errors)}`).toBeUndefined()
    expect(rust.data?.appFiles).toEqual([])
    expect(android.data?.appFiles).toEqual([])
  })
})
