import { expect, it } from 'vitest'
import { WebSocketCrypto } from '../../src/lib/api/websocket-crypto'
import { server, text } from '../helpers/websocket-server'

const key = new Uint8Array(32).fill(7)
function fixture(path = '/screen', client = 'browser') {
  const crypto = new WebSocketCrypto(client, key, path)
  const peer = server(key, crypto.registration, path, client)
  crypto.acknowledge(peer.ack)
  return { crypto, peer }
}

it('uses independent counters starting at one with no clock-dependent registration', () => {
  const { crypto, peer } = fixture()
  expect(peer.init).toEqual({ clientNonce: expect.stringMatching(/^[0-9a-f]{64}$/) })
  for (let index = 1n; index <= 3n; index++) {
    const frame = crypto.encode(text('control'))
    expect(frame.length).toBe(8 + 24 + text('control').length + 16)
    expect(new DataView(frame.buffer).getBigUint64(0)).toBe(index)
    expect(peer.decode(frame)).toEqual(text('control'))
  }
  expect(crypto.decode(peer.frame(1n, text('event')))).toEqual(text('event'))
})

it('rejects replay, gaps, zero, corruption, truncation and reflected direction without consuming a valid sequence', () => {
  const { crypto, peer } = fixture()
  const frame = peer.frame(1n, text('event'))
  for (let length = 0; length < frame.length; length++) expect(() => crypto.decode(frame.subarray(0, length))).toThrow()
  for (const sequence of [0n, 2n, 0xffffffffffffffffn]) expect(() => crypto.decode(peer.frame(sequence, text('event')))).toThrow()
  expect(() => crypto.decode(peer.frame(1n, text('event'), 'client'))).toThrow()
  const changed = frame.slice()
  changed[changed.length - 1]! ^= 1
  expect(() => crypto.decode(changed)).toThrow()
  expect(crypto.decode(frame)).toEqual(text('event'))
  expect(() => crypto.decode(frame)).toThrow()
  expect(crypto.decode(peer.frame(2n, text('next')))).toEqual(text('next'))
})

it('rejects old acknowledgements and old frames on reconnect and binds endpoint and identity', () => {
  const { crypto, peer } = fixture()
  expect(() => crypto.acknowledge(peer.ack)).toThrow()
  const next = new WebSocketCrypto('browser', key, '/screen')
  expect(() => next.acknowledge(peer.ack)).toThrow()
  const nextPeer = server(key, next.registration, '/screen', 'browser')
  next.acknowledge(nextPeer.ack)
  expect(() => next.decode(peer.frame(1n, text('old')))).toThrow()
  for (const [path, client] of [['/image_editor', 'browser'], ['/screen', 'another']]) {
    const other = fixture(path, client)
    expect(() => crypto.decode(other.peer.frame(1n, text('wrong')))).toThrow()
  }
  expect(next.decode(nextPeer.frame(1n, text('new')))).toEqual(text('new'))
})

it('requires handshake and permits the final u64 sequence once without wrapping', () => {
  const pending = new WebSocketCrypto('browser', key)
  expect(() => pending.encode(text('early'))).toThrow()
  const { crypto, peer } = fixture()
  const counters = crypto as unknown as { sent: bigint; received: bigint }
  counters.sent = 0xfffffffffffffffen
  counters.received = 0xfffffffffffffffen
  const frame = crypto.encode(text('last'))
  expect(new DataView(frame.buffer).getBigUint64(0)).toBe(0xffffffffffffffffn)
  expect(peer.decode(frame)).toEqual(text('last'))
  expect(() => crypto.encode(text('overflow'))).toThrow()
  expect(crypto.decode(peer.frame(0xffffffffffffffffn, text('last')))).toEqual(text('last'))
  expect(() => crypto.decode(peer.frame(0n, text('wrapped')))).toThrow()
})
