import { ref, shallowRef, onScopeDispose } from 'vue'
import { getGraphqlContextKey } from '@/lib/api/gql-client'
import { createApiToken, readSessions, renameSession, revokeSession, type SessionRecord } from './api'

export function useSessions() {
  const entries = shallowRef<SessionRecord[]>([])
  const loading = ref(false)
  const busy = ref(false)
  const error = ref('')
  let generation = 0

  function reset() {
    generation++
    entries.value = []
    loading.value = false
    busy.value = false
    error.value = ''
  }

  async function load() {
    const request = ++generation
    const context = getGraphqlContextKey()
    loading.value = true
    error.value = ''
    try {
      const result = await readSessions()
      if (request === generation && context === getGraphqlContextKey()) entries.value = result
    } catch (cause) {
      if (request === generation && context === getGraphqlContextKey()) error.value = cause instanceof Error ? cause.message : String(cause)
    } finally {
      if (request === generation) loading.value = false
    }
  }

  async function change<T>(action: () => Promise<T>): Promise<T | false> {
    if (busy.value) return false
    let request = generation
    const context = getGraphqlContextKey()
    busy.value = true
    error.value = ''
    try {
      const result = await action()
      if (request !== generation || context !== getGraphqlContextKey()) return false
      if (result === false) throw new Error('session_management.operation_failed')
      const refresh = load()
      request = generation
      await refresh
      return request === generation && context === getGraphqlContextKey() ? result : false
    } catch (cause) {
      if (request === generation && context === getGraphqlContextKey()) error.value = cause instanceof Error ? cause.message : String(cause)
      return false
    } finally {
      if (request === generation && context === getGraphqlContextKey()) busy.value = false
    }
  }

  onScopeDispose(reset)
  return {
    entries, loading, busy, error, load, reset,
    rename: (clientId: string, name: string) => change(() => renameSession(clientId, name.trim())),
    revoke: (clientId: string) => change(() => revokeSession(clientId)),
    create: (name: string) => change(() => createApiToken(name.trim())),
  }
}
