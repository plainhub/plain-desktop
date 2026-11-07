import { getApiBaseUrl, getApiHeaders, getLocalServerPort, proxyUrlFor } from './api'
import { FileAction, fileQuery } from './file-action'
import { getFileId, tokenToKey } from './file'
import { gqlFetch, nativeGqlFetch, type GqlResult } from './gql-client'
import { gqlFetchPeer, getPeerBaseUrl } from './peer-client'
import { appGQL, docsGQL, feedEntriesGQL, imagesGQL, notesGQL, videosGQL } from './query'
import type { GqlResultType, GqlVariablesType } from './graphql/operations'
import { buildTextFilter } from '@/lib/search'
import { isLocalMode } from '@/lib/device/local-mode'
import type { LoginPeer } from '@/lib/device/login-peers'
import type { AccessFeature } from '@/lib/interfaces'

export const searchDomains = ['images', 'videos', 'docs', 'feeds', 'notes'] as const
export type SearchDomain = (typeof searchDomains)[number]
type Image = GqlResultType<'imagesGQL'>['images'][number]
type Video = GqlResultType<'videosGQL'>['videos'][number]
type Doc = GqlResultType<'docsGQL'>['items'][number]
export type FileSearchHit =
  | { domain: 'images'; item: Image }
  | { domain: 'videos'; item: Video }
  | { domain: 'docs'; item: Doc }
export type SearchHit = FileSearchHit
  | { domain: 'feeds'; item: GqlResultType<'feedEntriesGQL'>['items'][number] }
  | { domain: 'notes'; item: GqlResultType<'notesGQL'>['notes'][number] }

export type SearchDevice = { id: string; name: string } & (
  | { target: 'current' }
  | { target: 'local' }
  | { target: 'peer'; peer: LoginPeer }
)

const access: Record<FileSearchHit['domain'], AccessFeature> = { images: 'IMAGES', videos: 'VIDEOS', docs: 'DOCS' }
export const searchPageSize = 24

export async function loadSearchClient(device: SearchDevice, foreground = false) {
  const local = device.target === 'local' || (device.target === 'current' && isLocalMode())
  const scope = local ? { mode: 'g' as const } : { mode: 'c' as const, id: getApiHeaders()['c-id'] }
  const base = device.target === 'peer' ? getPeerBaseUrl(device.peer)
    : device.target === 'local' ? `http://localhost:${getLocalServerPort()}` : getApiBaseUrl()
  async function execute<T>(document: string, variables?: Record<string, unknown>): Promise<T> {
    let result: GqlResult<T>
    if (device.target === 'peer') result = await gqlFetchPeer<T>(device.peer, document, variables)
    else if (device.target === 'local') result = await nativeGqlFetch<T>(document, variables)
    else result = await gqlFetch<T>(document, variables)
    if (result.errors?.length) throw new Error(result.errors[0].message)
    return result.data
  }
  const { app } = await execute<GqlResultType<'appGQL'>>(appGQL)
  const key = tokenToKey(app.urlToken)
  const fileId = (hit: FileSearchHit, foregroundThumbnail = false) =>
    getFileId(key, hit.item.path, hit.domain === 'docs' ? '' : hit.item.id, scope, foregroundThumbnail)
  const fileUrl = (id: string, query = '') => proxyUrlFor(base, `/fs?id=${encodeURIComponent(id)}${fileQuery(FileAction.Preview, query)}`)
  return {
    name: app.deviceName || device.name,
    canSearch: (domain: SearchDomain) => domain === 'feeds' || domain === 'notes' || app.access.includes(access[domain]),
    async page(domain: SearchDomain, text: string, offset: number): Promise<{ hits: SearchHit[]; total: number }> {
      const query = buildTextFilter(text)
      const paging = { offset, limit: searchPageSize }
      if (domain === 'feeds') {
        const data = await execute<GqlResultType<'feedEntriesGQL'>>(feedEntriesGQL, { ...paging, query })
        return { hits: data.items.map((item) => ({ domain, item })), total: data.total }
      }
      if (domain === 'notes') {
        const data = await execute<GqlResultType<'notesGQL'>>(notesGQL, { ...paging, query: `${query} trash:false` })
        return { hits: data.notes.map((item) => ({ domain, item })), total: data.noteCount }
      }
      const variables: GqlVariablesType<'imagesGQL'> = { ...paging, query: `${query} trash:false`, sortBy: 'DATE_DESC' }
      if (domain === 'images') {
        const data = await execute<GqlResultType<'imagesGQL'>>(imagesGQL, variables)
        return { hits: data.images.map((item) => ({ domain, item })), total: data.imageCount }
      }
      if (domain === 'videos') {
        const data = await execute<GqlResultType<'videosGQL'>>(videosGQL, variables)
        return { hits: data.videos.map((item) => ({ domain, item })), total: data.videoCount }
      }
      const data = await execute<GqlResultType<'docsGQL'>>(docsGQL, variables)
      return { hits: data.items.map((item) => ({ domain, item })), total: data.total }
    },
    fileId: (hit: FileSearchHit) => fileId(hit),
    url: (hit: FileSearchHit) => fileUrl(fileId(hit)),
    thumbnailUrl(hit: FileSearchHit) {
      const id = fileId(hit, __IS_TAURI__ && local && foreground && hit.domain === 'videos')
      return fileUrl(id, '&size=sm&v=' + encodeURIComponent(hit.item.updatedAt))
    },
  }
}
