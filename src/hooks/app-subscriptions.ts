import { isLocalMode } from '@/lib/device/local-mode'
import { onMounted, onUnmounted, watch } from 'vue'
import { useTempStore } from '@/stores/temp'
import { getGraphqlContextKey } from '@/lib/api/gql-client'
import { getCurrentAuthToken } from '@/lib/device/current'
import { graphqlSubscriptions as client } from '@/lib/api/subscription-client'
import { subscriptionOperations } from '@/lib/api/subscription-operations'

export function useAppSubscriptions() {
  const store = useTempStore()
  let stop: (() => void) | undefined
  onMounted(() => {
    stop = watch(() => ({
      context: getGraphqlContextKey(),
      authenticated: isLocalMode() || Boolean(getCurrentAuthToken()),
      capabilities: store.app.capabilities ?? [],
      permissions: store.app.permissions ?? [],
    }), (now, previous) => {
      if (!now.authenticated) { client.stop(); return }
      if (previous && now.context !== previous.context) client.stop()
      client.setOperations(subscriptionOperations.filter(operation =>
        (!operation.capability || now.capabilities.some(capability => capability === operation.capability)) &&
        operation.permissions.every(permission => now.permissions.some(grant => grant === permission)),
      ))
      if (!previous || !previous.authenticated || now.context !== previous.context) client.start()
    }, { immediate: true, deep: true })
  })
  onUnmounted(() => { stop?.(); client.stop() })
}
