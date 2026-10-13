import { randomUUID } from 'node:crypto'
import { on } from 'node:events'
import WebSocket from 'ws'
import { describe, expect, it } from 'vitest'
import { decrypt, encode, encrypt, gql, post, targets, tls } from './helpers'

const noteFields = 'id title content deletedAt'
const channelFields = 'id name ownerId members { peerId status } status version'
const itemFields = 'id fromId toId channelId content status'

describe.concurrent.each(targets)('$name', (target) => {
  it('device identity', async () => {
    const { app } = await gql(target, 'query { app { clientId deviceName deviceType } }')
    expect(app.clientId).toBeTruthy()
    expect(app.deviceName).toEqual(expect.any(String))
    expect(['PHONE', 'TABLET', 'COMPUTER', 'NAS', 'TV', 'OTHER']).toContain(app.deviceType)
  })

  it.each([
    ['peers', 'peers { id name online }'],
    ['feeds', 'feeds { id name url }'],
    ['feedEntries', 'feedEntries(query: "", offset: 0, limit: 1) { id title }'],
    ['bookmarks', 'bookmarks { id title url }'],
    ['bookmarkGroups', 'bookmarkGroups { id name }'],
    ['tags', 'tags(type: NOTE) { id name count }'],
    ['favoriteFolders', 'favoriteFolders { rootPath relativePath }'],
    ['latestChatItems', 'latestChatItems { id content status }'],
    ['appFiles', 'appFiles(query: "", offset: 0, limit: 1) { id fileName size }'],
    ...['images', 'videos', 'audios', 'docs'].map((field) => [field, `${field}(query: "", offset: 0, limit: 1, sortBy: DATE_DESC) { id title size }`]),
  ])('%s query', async (field, selection) => {
    const data = await gql(target, `query { ${selection} }`)
    expect(Array.isArray(data[field!])).toBe(true)
  })

  it('notes: create → read → update → trash → restore → delete', async () => {
    const title = `api-test-${randomUUID()}`
    let id = ''
    try {
      const { createNote: note } = await gql(target, `mutation($input: NoteInput!) { createNote(input: $input) { ${noteFields} } }`, { input: { title, content: 'API test 内容' } })
      id = note.id
      expect(id).toBeTruthy()
      expect(note).toMatchObject({ title, content: 'API test 内容', deletedAt: null })
      expect((await gql(target, `query($id: ID!) { note(id: $id) { ${noteFields} } }`, { id })).note).toEqual(note)
      const { updateNote } = await gql(target, `mutation($id: ID!, $input: NoteInput!) { updateNote(id: $id, input: $input) { ${noteFields} } }`, { id, input: { title, content: 'updated' } })
      expect(updateNote.content).toBe('updated')
      const query = `ids:${id}`
      const page = await gql(target, `query($query: String!) { notes(query: $query, offset: 0, limit: 1) { ${noteFields} } noteCount(query: $query) }`, { query })
      expect(page.notes).toEqual([updateNote])
      expect(page.noteCount).toBe(1)
      expect((await gql(target, 'mutation($query: String!) { trashNotes(query: $query) { affectedCount } }', { query })).trashNotes.affectedCount).toBe(1)
      expect((await gql(target, 'query($query: String!) { noteCount(query: $query) }', { query })).noteCount).toBe(0)
      expect((await gql(target, 'mutation($query: String!) { restoreNotes(query: $query) { affectedCount } }', { query })).restoreNotes.affectedCount).toBe(1)
      expect((await gql(target, `query($id: ID!) { note(id: $id) { ${noteFields} } }`, { id })).note).toEqual(updateNote)
    } finally {
      if (id) {
        await gql(target, 'mutation($query: String!) { trashNotes(query: $query) { affectedCount } }', { query: `ids:${id}` })
        expect((await gql(target, 'mutation($query: String!) { deleteNotes(query: $query) { affectedCount } }', { query: `ids:${id}` })).deleteNotes.affectedCount).toBe(1)
        expect((await gql(target, 'query($id: ID!) { note(id: $id) { id } }', { id })).note).toBeNull()
      }
    }
  })

  it('channels: create → read → rename → delete', async () => {
    const name = `api-test-${randomUUID()}`
    let id = ''
    try {
      const { createChatChannel: channel } = await gql(target, `mutation($name: String!) { createChatChannel(name: $name) { ${channelFields} } }`, { name })
      id = channel.id
      expect(id).toBeTruthy()
      expect(channel).toMatchObject({ name, version: 1, status: 'JOINED' })
      expect(channel.members).toContainEqual({ peerId: channel.ownerId, status: 'JOINED' })
      expect((await gql(target, `query { chatChannels { ${channelFields} } }`)).chatChannels).toContainEqual(channel)
      const renamed = await gql(target, 'mutation($id: ID!, $name: String!) { updateChatChannel(id: $id, name: $name) { id name version } }', { id, name: `${name}-renamed` })
      expect(renamed.updateChatChannel).toEqual({ id, name: `${name}-renamed`, version: 2 })
    } finally {
      if (id) {
        expect((await gql(target, 'mutation($id: ID!) { deleteChatChannel(id: $id) }', { id })).deleteChatChannel).toBe(true)
        expect((await gql(target, 'query { chatChannels { id } }')).chatChannels.some((channel: { id: string }) => channel.id === id)).toBe(false)
      }
    }
  })

  it('messages: send → read → delete', async () => {
    const content = JSON.stringify({ type: 'TEXT', value: { text: `api-test-${randomUUID()}` } })
    const ids: string[] = []
    try {
      const { sendChatItem: items } = await gql(target, `mutation($content: String!) { sendChatItem(target: "local", content: $content) { ${itemFields} } }`, { content })
      ids.push(...items.map((item: { id: string }) => item.id))
      expect(items).toHaveLength(1)
      expect(items[0]).toMatchObject({ content, fromId: 'me', toId: 'local', channelId: null, status: 'SENT' })
      expect((await gql(target, `query($id: ID!) { chatItem(id: $id) { ${itemFields} } }`, { id: ids[0] })).chatItem).toEqual(items[0])
    } finally {
      for (const id of ids) {
        expect((await gql(target, 'mutation($id: ID!) { deleteChatItem(id: $id) }', { id })).deleteChatItem).toBe(true)
        expect((await gql(target, 'query($id: ID!) { chatItem(id: $id) { id } }', { id })).chatItem).toBeNull()
      }
    }
  })

  it('unknown fields fail instead of counting HTTP 200 as success', async () => {
    await expect(gql(target, 'query { apiTestUnknownField }')).rejects.toThrow(/unknown|cannot query/i)
  })

  it.each([
    ['missing note', 'mutation($id: ID!) { updateNote(id: $id, input: { title: "missing", content: "" }) { id } }'],
    ['missing channel', 'mutation($id: ID!) { updateChatChannel(id: $id, name: "missing") { id } }'],
  ])('%s update fails', async (_name, query) => {
    await expect(gql(target, query!, { id: `api-test-${randomUUID()}` })).rejects.toThrow(/not found/i)
  })

  it('replayed requests fail', async () => {
    const frame = encode(target, 'query { __typename }')
    expect((await post(target, frame)).status).toBe(200)
    expect((await post(target, frame)).status).toBe(400)
  })

  it('WebSocket receives the event for its own message', async () => {
    const url = new URL(`${target.url.replace(/\/+$/, '')}/`)
    url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
    url.searchParams.set('cid', target.clientId)
    const socket = new WebSocket(url, { ...tls(target), handshakeTimeout: 10000 })
    const incoming = on(socket, 'message', { signal: AbortSignal.timeout(10000), close: ['close'] })
    const content = JSON.stringify({ type: 'TEXT', value: { text: `api-test-${randomUUID()}` } })
    const ids: string[] = []
    socket.once('open', () => socket.send(encrypt(target, '{}')))
    try {
      for await (const [frame] of incoming) {
        const separator = frame.indexOf(0)
        const name = frame.subarray(0, separator).toString('utf8')
        if (!['ONLINE_CLIENTS_UPDATED', 'MESSAGE_CREATED'].includes(name)) continue
        const payload = JSON.parse(decrypt(target, frame.subarray(separator + 1)).toString('utf8'))
        if (name === 'ONLINE_CLIENTS_UPDATED' && !ids.length && payload.includes(target.clientId)) {
          const { sendChatItem } = await gql(target, 'mutation($content: String!) { sendChatItem(target: "local", content: $content) { id } }', { content })
          ids.push(...sendChatItem.map((item: { id: string }) => item.id))
          expect(ids).toHaveLength(1)
        }
        if (name === 'MESSAGE_CREATED' && payload.some((item: { id: string }) => ids.includes(item.id))) {
          expect(payload).toEqual(expect.arrayContaining([expect.objectContaining({ id: ids[0], content })]))
          return
        }
      }
      throw new Error('WebSocket closed without the expected message event')
    } finally {
      if (socket.readyState !== WebSocket.CLOSED) {
        const closed = new Promise<void>((resolve) => socket.once('close', () => resolve()))
        socket.terminate()
        await closed
      }
      await incoming.return?.()
      for (const id of ids) await gql(target, 'mutation($id: ID!) { deleteChatItem(id: $id) }', { id })
    }
  })
})
