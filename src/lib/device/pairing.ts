import { ref } from 'vue'
import { nativeGraphql } from '@/lib/api/native-client'
import { graphqlSubscriptions } from '@/lib/api/subscription-client'

export interface PairingState {
  deviceId: string | null
  generation: string
  mode: 'CHAT' | 'MANAGE'
  phase: string
  error: string | null
  verificationCode?: string | null
}
export interface PairingInput {
  id?: string
  host?: string
  mode?: 'CHAT' | 'MANAGE'
  password?: string
  generation?: string
}
const fields = `deviceId generation mode phase error verificationCode`
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
  const receive = (value: PairingState) => {
    if (value.generation !== generation || state.value && terminal.has(state.value.phase)) return
    const phases = ['STARTING', 'PASSWORD_REQUIRED', 'AUTHENTICATING', 'PAIRING', 'WAITING_CONFIRMATION', 'COMMITTING', 'SUCCEEDED', 'FAILED', 'CANCELLED']
    if (state.value && phases.indexOf(value.phase) >= 0 && phases.indexOf(state.value.phase) > phases.indexOf(value.phase)) return
    state.value = value
  }
  const reconcile = async (connected: boolean) => {
    if (!connected || !generation || disposed) return
    const result = await execute<{ pairingStates: PairingState[] }>(`query { pairingStates { ${fields} } }`)
    const value = result.pairingStates.find((s) => s.generation === generation)
    if (value) receive(value)
  }
  const refresh = () => { void reconcile(true).catch(error => console.error('Pairing state refresh failed', error)) }
  const stopChanges = graphqlSubscriptions.subscribe('pairingChanged', refresh)
  const stopAttached = graphqlSubscriptions.onAttached(refresh)
  return {
    state,
    async start(input: PairingInput) {
      generation = undefined
      state.value = null
      const result = await execute<{ pairDevice: PairingState }>(`mutation($input: PairingDeviceInput!) { pairDevice(input: $input) { ${fields} } }`, { input })
      generation = result.pairDevice.generation
      state.value = result.pairDevice
      refresh()
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
      stopChanges()
      stopAttached()
      if (cancel) void execute('mutation($generation: String!) { cancelPairing(generation: $generation) }', { generation }).catch(() => {})
    },
  }
}
