import { ref } from 'vue'
import { subscribeNativeEvents } from '@/lib/api/native-events'
import { parseWebSocketData } from '@/lib/api/sjcl-arraybuffer'
import { getLocalToken } from '@/lib/api/api'
import { tokenToKey } from '@/lib/api/file'
import { chachaDecrypt } from '@/lib/api/crypto'
import { nativeGraphql } from '@/lib/api/native-client'
import emitter from '@/plugins/eventbus'

export interface PairingState {
  deviceId: string | null
  generation: string
  mode: 'CHAT' | 'MANAGE'
  phase: string
  error: string | null
}
export interface PairingInput {
  id?: string
  host?: string
  mode?: 'CHAT' | 'MANAGE'
  password?: string
  generation?: string
}
const fields = `deviceId generation mode phase error`
const terminal = new Set(['SUCCEEDED', 'FAILED', 'CANCELLED'])

async function execute<T>(query: string, variables?: Record<string, unknown>): Promise<T> {
  const result = await nativeGraphql<{ data: T; errors?: { message: string }[] }>({ query, variables }, 30000)
  if (result.errors?.length) throw new Error(result.errors[0].message)
  return result.data
}

export function createPairing() {
  const state = ref<PairingState | null>(null)
  let generation: string | undefined
  let disposed = false
  const early = new Map<string, PairingState>()
  const receive = (value: PairingState) => {
    if (!generation) { if (early.size < 128) early.set(value.generation, value); return }
    if (value.generation !== generation || state.value && terminal.has(state.value.phase)) return
    const phases = ['STARTING', 'PASSWORD_REQUIRED', 'AUTHENTICATING', 'PAIRING', 'WAITING_CONFIRMATION', 'COMMITTING', 'SUCCEEDED', 'FAILED', 'CANCELLED']
    if (state.value && phases.indexOf(value.phase) >= 0 && phases.indexOf(state.value.phase) > phases.indexOf(value.phase)) return
    state.value = value
  }
  const stream = subscribeNativeEvents((frame) => {
    const event = parseWebSocketData(frame)
    if (event.type === 'PAIRING_STATE_CHANGED') receive(JSON.parse(chachaDecrypt(tokenToKey(getLocalToken()), event.data)))
  }, () => {
    if (state.value && !terminal.has(state.value.phase)) state.value = { ...state.value, phase: 'FAILED', error: 'failed_connect' }
  })
  const reconcile = async (connected: boolean) => {
    if (!connected || !generation || disposed) return
    const result = await execute<{ pairingStates: PairingState[] }>(`query { pairingStates { ${fields} } }`)
    const value = result.pairingStates.find((s) => s.generation === generation)
    if (value) receive(value)
  }
  const reconnect = (connected: boolean) => { void reconcile(connected).catch(() => {}) }
  emitter.on('app_socket_connection_changed', reconnect)
  return {
    state,
    async start(input: PairingInput) {
      generation = undefined
      state.value = null
      early.clear()
      const result = await execute<{ pairDevice: PairingState }>(`mutation($input: PairingDeviceInput!) { pairDevice(input: $input) { ${fields} } }`, { input })
      generation = result.pairDevice.generation
      state.value = early.get(generation) ?? result.pairDevice
      early.clear()
      if (disposed) await execute('mutation($generation: String!) { cancelPairing(generation: $generation) }', { generation })
    },
    async submit(password: string) {
      if (!generation) return
      await execute(`mutation($input: PairingDeviceInput!) { pairDevice(input: $input) { ${fields} } }`, { input: { generation, password } })
    },
    async cancel() {
      if (generation && state.value && !terminal.has(state.value.phase)) await execute('mutation($generation: String!) { cancelPairing(generation: $generation) }', { generation })
    },
    dispose() {
      disposed = true
      const cancel = generation && state.value && !terminal.has(state.value.phase)
      if (cancel && state.value) state.value = { ...state.value, phase: 'CANCELLED' }
      stream.close()
      emitter.off('app_socket_connection_changed', reconnect)
      if (cancel) void execute('mutation($generation: String!) { cancelPairing(generation: $generation) }', { generation }).catch(() => {})
    },
  }
}
