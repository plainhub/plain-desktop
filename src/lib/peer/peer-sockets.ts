import { watch } from 'vue'
import emitter from '@/plugins/eventbus'
import { deviceBaseUrl } from '@/lib/api/api'
import { chachaEncrypt, chachaDecrypt, bitArrayToUint8Array } from '@/lib/api/crypto'
import { tokenToKey } from '@/lib/api/file'
import { parseWebSocketData } from '@/lib/api/sjcl-arraybuffer'
import { openSocket } from '@/lib/api/http'
import { loginPeers, peerHost, findLoginPeer, type LoginPeer } from '@/lib/device/login-peers'
import { get as prefsGet } from '@/lib/prefs'

interface PeerSocket {
  ws: WebSocket
  retryTime: number
  timer?: ReturnType<typeof setTimeout>
}

const sockets = new Map<string, PeerSocket>()
let started = false

function connectPeer(peer: LoginPeer) {
  const state: PeerSocket = { ws: undefined as unknown as WebSocket, retryTime: 1000 }
  sockets.set(peer.id, state)

  const key = tokenToKey(peer.token)
  const scheme = deviceBaseUrl(peerHost(peer)).startsWith('https') ? 'wss' : 'ws'
  const url = `${scheme}://${peerHost(peer)}/?cid=${prefsGet('client_id', '')}`

  const dial = () => {
    const ws = openSocket(url, peer.id)
    state.ws = ws
    ws.onopen = () => {
      state.retryTime = 1000
      ws.send(bitArrayToUint8Array(chachaEncrypt(key, new Date().getTime().toString())))
    }
    ws.onmessage = async (event: MessageEvent) => {
      const r = parseWebSocketData(await event.data.arrayBuffer())
      try {
        const json = chachaDecrypt(key, r.data)
        const data = json ? JSON.parse(json) : null
        if (data && findLoginPeer(peer.id)) {
          emitter.emit('peer_ws_event', { peerId: peer.id, type: r.type, data })
        }
      } catch (ex) {
        console.error(ex)
      }
    }
    ws.onclose = () => {
      if (!findLoginPeer(peer.id)) return
      state.timer = setTimeout(dial, state.retryTime)
      state.retryTime = Math.min(state.retryTime * 2, 30000)
    }
    ws.onerror = () => ws.close()
  }
  dial()
}

function disconnectPeer(peerId: string) {
  const state = sockets.get(peerId)
  if (!state) return
  sockets.delete(peerId)
  if (state.timer) clearTimeout(state.timer)
  state.ws?.close()
}

function syncSockets() {
  const alive = new Set(loginPeers.value.map((p) => p.id))
  for (const id of [...sockets.keys()]) {
    if (!alive.has(id)) disconnectPeer(id)
  }
  for (const p of loginPeers.value) {
    if (!sockets.has(p.id)) connectPeer(p)
  }
}

export function startPeerSockets() {
  if (started || !__IS_TAURI__) return
  started = true
  syncSockets()
  watch(loginPeers, syncSockets)
}

export function stopPeerSockets() {
  if (!started) return
  started = false
  for (const id of [...sockets.keys()]) disconnectPeer(id)
}
