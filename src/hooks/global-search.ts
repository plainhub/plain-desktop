import { computed, ref, shallowRef, watch, type InjectionKey, type Ref } from 'vue'
import type { IApp } from '@/lib/interfaces'
import { getDesktopClientId, getRemoteClientId } from '@/lib/device/client-id'
import { loginPeers } from '@/lib/device/login-peers'
import { GqlError } from '@/lib/api/gql-client'
import { loadSearchClient, searchDomains, type SearchDevice, type SearchDomain, type SearchHit } from '@/lib/api/global-search'

type Client = Awaited<ReturnType<typeof loadSearchClient>>
type Page = { domain: SearchDomain; hits: SearchHit[]; total: number; loading: boolean; failed: boolean }
export type SearchGroup = {
  device: SearchDevice
  client?: Client
  status: 'loading' | 'ready' | 'offline' | 'denied' | 'failed'
  pages: Page[]
}

export function searchGroupTotal(group: SearchGroup) {
  return group.pages.reduce((total, page) => total + page.total, 0)
}

export function useGlobalSearch(app: Ref<IApp>, localName: () => string) {
  const domain = ref<SearchDomain | 'all'>('all')
  const request = shallowRef<{ text: string; foreground: boolean }>()
  const selected = ref<string[] | null>(null)
  const groups = ref<SearchGroup[]>([])
  const devices = computed<SearchDevice[]>(() => {
    if (!app.value?.clientId) return []
    const current: SearchDevice = { id: app.value.clientId, name: app.value.deviceName, target: 'current' }
    const sources: SearchDevice[] = [current]
    const remoteId = getRemoteClientId()
    if (remoteId) {
      const localId = getDesktopClientId()
      if (localId && localId !== current.id) sources.push({ id: localId, name: localName(), target: 'local' })
    }
    for (const peer of loginPeers.value) {
      if (peer.id !== current.id && peer.id !== remoteId) sources.push({ id: peer.id, name: peer.name, target: 'peer', peer })
    }
    return sources
  })
  const targets = computed(() => devices.value.length <= 1
    ? devices.value
    : devices.value.filter((device) => selected.value === null || selected.value.includes(device.id)))
  const total = computed(() => groups.value.reduce((sum, group) => sum + searchGroupTotal(group), 0))

  function select(id: string, checked: boolean) {
    const ids = targets.value.map((device) => device.id)
    selected.value = checked ? [...ids, id] : ids.filter((value) => value !== id)
  }

  function submit(text: string, foreground = false) {
    const query = text.trim()
    request.value = query ? { text: query, foreground } : undefined
  }

  async function loadPage(group: SearchGroup, page: Page, text: string) {
    if (!group.client || page.loading) return
    page.loading = true
    page.failed = false
    try {
      const result = await group.client.page(page.domain, text, page.hits.length)
      page.hits.push(...result.hits)
      page.total = result.total
    } catch {
      page.failed = true
    } finally {
      page.loading = false
    }
  }

  async function loadDevice(group: SearchGroup, query: NonNullable<typeof request.value>, domains: readonly SearchDomain[]) {
    group.status = 'loading'
    try {
      group.client = await loadSearchClient(group.device, query.foreground)
      if (!groups.value.includes(group)) return
      group.pages = domains.filter(group.client.canSearch).map((domain) => ({ domain, hits: [], total: 0, loading: false, failed: false }))
      if (!group.pages.length) {
        group.status = 'denied'
        return
      }
      group.status = 'ready'
      await Promise.all(group.pages.map((page) => loadPage(group, page, query.text)))
    } catch (error) {
      if (error instanceof GqlError && (error.status === 401 || error.status === 403)) group.status = 'denied'
      else if (error instanceof GqlError && !error.status && group.device.target === 'peer' && group.device.peer.online === false) group.status = 'offline'
      else group.status = 'failed'
    }
  }

  watch([request, domain, targets], ([query, type, devices]) => {
    groups.value = query ? devices.map((device) => ({ device, status: 'loading', pages: [] })) : []
    if (!query) return
    const domains = type === 'all' ? searchDomains : [type]
    for (const group of groups.value) void loadDevice(group, query, domains)
  })

  return {
    devices, targets, domain, request, groups, total, select, submit,
    selectAll: (checked: boolean) => { selected.value = checked ? null : [] },
    loadMore: (group: SearchGroup, page: Page) => {
      if (request.value) void loadPage(group, page, request.value.text)
    },
    retry: (group: SearchGroup) => {
      if (request.value) void loadDevice(group, request.value, domain.value === 'all' ? searchDomains : [domain.value])
    },
    reset() {
      request.value = undefined
      selected.value = null
    },
  }
}

export type GlobalSearch = ReturnType<typeof useGlobalSearch>
export const globalSearchKey: InjectionKey<{ search: GlobalSearch; sidebarCollapsed: Ref<boolean> }> = Symbol('globalSearch')
