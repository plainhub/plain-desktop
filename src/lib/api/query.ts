import { ref, watch, nextTick, getCurrentInstance, onActivated, onDeactivated, type Ref } from 'vue'
import { gqlFetch, GqlError } from './gql-client'
import {
  chatItemFragment,
  chatChannelFragment,
  messageFragment,
  messageConversationFragment,
  messageConversationWithAddressesFragment,
  contactFragment,
  callFragment,
  imageFragment,
  videoFragment,
  audioFragment,
  fileFragment,
  appFragment,
  tagFragment,
  noteFragment,
  feedFragment,
  feedEntryFragment,
  packageFragment,
  tagSubFragment,
  notificationFragment,
  deviceInfoFragment,
  bookmarkFragment,
  bookmarkGroupFragment,
  docFragment,
} from './fragments'

// --- Query Wrappers ---

function getErrorMessage(e: any): string {
  if (e instanceof GqlError) {
    if (e.status === 403) return 'desktop_access_disabled'
    return e.message
  }
  return 'network_error'
}

function resolveVars(variables: any): Record<string, any> | undefined {
  if (!variables) return undefined
  return typeof variables === 'function' ? variables() : variables
}

export interface InitQueryParams<TResult> {
  handle: (data: TResult, error: string, context?: QueryResponseContext) => void
  document: string
  variables?: any
  options?: any
}

export interface QueryFetchOptions {
  force?: boolean
  latest?: boolean
  meta?: unknown
}

export interface QueryResponseContext {
  variables?: Record<string, any>
  requestId: number
  meta?: unknown
}

export function initQuery<TResult = any>(params: InitQueryParams<TResult>) {
  const loading = ref(false)
  const result = ref<TResult>()

  async function execute(vars?: Record<string, any>) {
    loading.value = true
    try {
      const v = vars ?? resolveVars(params.variables)
      const r = await gqlFetch<TResult>(params.document, v)
      if (r.errors?.length) {
        params.handle(r.data, r.errors[0].message)
      } else {
        result.value = r.data
        params.handle(r.data, '')
      }
    } catch (e: any) {
      params.handle(undefined as any, getErrorMessage(e))
    } finally {
      loading.value = false
    }
  }

  execute()

  if (typeof params.variables === 'function') {
    // Guard watcher so deactivated keep-alive instances don't fire queries
    // when the shared reactive route changes.
    // nextTick defers execution until after KeepAlive lifecycle hooks complete.
    let active = true
    const inst = getCurrentInstance()
    if (inst) {
      onDeactivated(() => { active = false })
      onActivated(() => { active = true })
    }
    watch(params.variables, async () => {
      await nextTick()
      if (active) execute()
    }, { deep: true })
  }

  return { loading, result, refetch: execute }
}

export function initLazyQuery<TResult = any>(params: InitQueryParams<TResult>) {
  const loading = ref(false)
  const result = ref<TResult>()
  let requestSequence = 0
  let latestSequence = 0
  let activeRegularRequests = 0
  let activeLatestRequest: number | undefined

  async function doFetch(vars?: Record<string, any>, options: QueryFetchOptions = {}) {
    const requestId = ++requestSequence
    const latestRequestId = options.latest ? ++latestSequence : undefined
    const resolved = vars ?? resolveVars(params.variables)
    const v = resolved ? { ...resolved } : undefined
    const context: QueryResponseContext = { variables: v, requestId, meta: options.meta }
    if (options.latest) activeLatestRequest = requestId
    else activeRegularRequests++
    loading.value = true
    try {
      const r = options.force
        ? await gqlFetch<TResult>(params.document, v, { fresh: true })
        : await gqlFetch<TResult>(params.document, v)
      if (options.latest && latestRequestId !== latestSequence) return
      if (r.errors?.length) {
        params.handle(r.data, r.errors[0].message, context)
      } else {
        result.value = r.data
        params.handle(r.data, '', context)
      }
    } catch (e: any) {
      if (!options.latest || latestRequestId === latestSequence) {
        params.handle(undefined as any, getErrorMessage(e), context)
      }
    } finally {
      if (options.latest) {
        if (activeLatestRequest === requestId) activeLatestRequest = undefined
      } else {
        activeRegularRequests--
      }
      loading.value = activeRegularRequests > 0 || activeLatestRequest !== undefined
    }
  }

  return { loading, result, fetch: doFetch }
}

// --- GraphQL Query Definitions ---

export const chatItemsGQL = `
  query ($id: String!) {
    chatItems(id: $id) {
      ...ChatItemFragment
    }
  }
  ${chatItemFragment}
`

export const chatItemGQL = `
  query ($id: String!) {
    chatItem(id: $id) {
      ...ChatItemFragment
    }
  }
  ${chatItemFragment}
`

export const peersGQL = `
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

export const latestChatItemsGQL = `
  query {
    latestChatItems {
      ...ChatItemFragment
    }
  }
  ${chatItemFragment}
`

export const appFilesGQL = `
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

export const chatChannelsGQL = `
  query {
    chatChannels {
      ...ChatChannelFragment
    }
  }
  ${chatChannelFragment}
`

export const fileInfoGQL = `
  query ($id: ID!, $path: String!, $fileName: String!) {
    fileInfo(id: $id, path: $path, fileName: $fileName) {
      ... on FileInfo {
        path
        updatedAt
        size
        tags {
          ...TagSubFragment
        }
      }
      data {
        ... on ImageFileInfo {
          width
          height
          location {
            latitude
            longitude
          }
        }
        ... on VideoFileInfo {
          duration
          width
          height
          location {
            latitude
            longitude
          }
        }
        ... on AudioFileInfo {
          duration
          location {
            latitude
            longitude
          }
        }
      }
    }
  }
  ${tagSubFragment}
`

export const smsGQL = `
  query sms($offset: Int!, $limit: Int!, $query: String!) {
    sms(offset: $offset, limit: $limit, query: $query) {
      ...MessageFragment
    }
    smsCount(query: $query)
  }
  ${messageFragment}
`

export const simsGQL = `
  query {
    sims {
      id
      label
      number
      subscriptionId
    }
  }
`

export const smsConversationsGQL = `
  query smsConversations($offset: Int!, $limit: Int!, $query: String!) {
    smsConversations(offset: $offset, limit: $limit, query: $query) {
      ...MessageConversationFragment
    }
    smsConversationCount(query: $query)
  }
  ${messageConversationFragment}
`

export const smsConversationsWithAddressesGQL = `
  query smsConversations($offset: Int!, $limit: Int!, $query: String!) {
    smsConversations(offset: $offset, limit: $limit, query: $query) {
      ...MessageConversationWithAddressesFragment
    }
    smsConversationCount(query: $query)
  }
  ${messageConversationWithAddressesFragment}
`

export const contactsGQL = `
  query contacts($offset: Int!, $limit: Int!, $query: String!) {
    contacts(offset: $offset, limit: $limit, query: $query) {
      ...ContactFragment
    }
    contactCount(query: $query)
  }
  ${contactFragment}
`

export const homeStatsGQL = `
  query homeStats($mediaQuery: String!) {
    smsCount(query: "")
    contactCount(query: "")
    callCount(query: "")
    imageCount(query: $mediaQuery)
    audioCount(query: $mediaQuery)
    videoCount(query: $mediaQuery)
    packageCount(query: "")
    noteCount(query: "")
    docCount(query: "")
    feedEntryCount(query: "")
    mounts {
      id
      path
      mountPoint
      totalBytes
      freeBytes
      driveType
    }
  }
`

export const contactSourcesGQL = `
  query {
    contactSources {
      name
      type
    }
  }
`

export const callsGQL = `
  query calls($offset: Int!, $limit: Int!, $query: String!) {
    calls(offset: $offset, limit: $limit, query: $query) {
      ...CallFragment
    }
    callCount(query: $query)
  }
  ${callFragment}
`

export const imagesGQL = `
  query images($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    images(offset: $offset, limit: $limit, query: $query, sortBy: $sortBy) {
      ...ImageFragment
    }
    imageCount(query: $query)
  }
  ${imageFragment}
`

export const imageSearchStatusGQL = `
  query {
    imageSearchStatus {
      status
      downloadProgress
      errorMessage
      modelSize
      modelDir
      isIndexing
      totalImages
      indexedImages
    }
  }
`

export const videosGQL = `
  query videos($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    videos(offset: $offset, limit: $limit, query: $query, sortBy: $sortBy) {
      ...VideoFragment
    }
    videoCount(query: $query)
  }
  ${videoFragment}
`

export const audiosGQL = `
  query audios($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    items: audios(offset: $offset, limit: $limit, query: $query, sortBy: $sortBy) {
      ...AudioFragment
    }
    total: audioCount(query: $query)
  }
  ${audioFragment}
`

export const filesGQL = `
  query files($root: String!, $offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    files(root: $root, offset: $offset, limit: $limit, query: $query, sortBy: $sortBy) {
      ...FileFragment
    }
  }
  ${fileFragment}
`

export const recentFilesGQL = `
  query recentFiles {
    recentFiles {
      ...FileFragment
    }
  }
  ${fileFragment}
`

export const mountsGQL = `
  query {
    mounts {
      id
      name
      path
      mountPoint
      fsType
      totalBytes
      usedBytes
      freeBytes
      remote
      alias
      driveType
      diskID
    }
  }
`

export const appGQL = `
  query {
    app {
      ...AppFragment
    }
  }
  ${appFragment}
`

export const tagsGQL = `
  query tags($type: DataType!) {
    tags(type: $type) {
      ...TagFragment
    }
  }
  ${tagFragment}
`

export const mediaBucketsGQL = `
  query mediaBuckets($type: DataType!) {
    mediaBuckets(type: $type) {
      id
      name
      itemCount
      topItems
    }
  }
`

export const notesGQL = `
  query notes($offset: Int!, $limit: Int!, $query: String!) {
    notes(offset: $offset, limit: $limit, query: $query) {
      id
      title
      deletedAt
      createdAt
      updatedAt
      tags {
        ...TagSubFragment
      }
    }
    noteCount(query: $query)
  }
  ${tagSubFragment}
`

export const noteGQL = `
  query note($id: ID!) {
    note(id: $id) {
      ...NoteFragment
    }
  }
  ${noteFragment}
`

export const feedsGQL = `
  query {
    feeds {
      ...FeedFragment
    }
  }
  ${feedFragment}
`

export const feedEntriesGQL = `
  query feedEntries($offset: Int!, $limit: Int!, $query: String!) {
    items: feedEntries(offset: $offset, limit: $limit, query: $query) {
      id
      title
      url
      image
      author
      feedId
      rawId
      publishedAt
      createdAt
      updatedAt
      tags {
        ...TagSubFragment
      }
    }
    total: feedEntryCount(query: $query)
  }
  ${tagSubFragment}
`

export const feedsTagsGQL = `
  query feedsTags($type: DataType!) {
    tags(type: $type) {
      ...TagFragment
    }
    feeds {
      ...FeedFragment
    }
  }
  ${feedFragment}
  ${tagFragment}
`

export const bucketsTagsGQL = `
  query bucketsTags($type: DataType!) {
    tags(type: $type) {
      ...TagFragment
    }
    mediaBuckets(type: $type) {
      id
      name
      itemCount
      topItems
    }
  }
  ${tagFragment}
`

export const feedEntryGQL = `
  query feedEntry($id: ID!) {
    feedEntry(id: $id) {
      ...FeedEntryFragment
      feed {
        ...FeedFragment
      }
    }
  }
  ${feedFragment}
  ${feedEntryFragment}
`

export const imageCountGQL = `
  query imageCount($query: String!) {
    total: imageCount(query: $query)
    trash: imageCount(query: "trash:true")
  }
`

export const audioCountGQL = `
  query audioCount($query: String!) {
    total: audioCount(query: $query)
    trash: audioCount(query: "trash:true")
  }
`

export const docsGQL = `
  query docs($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    items: docs(offset: $offset, limit: $limit, query: $query, sortBy: $sortBy) {
      ...DocFragment
    }
    total: docCount(query: $query)
  }
  ${docFragment}
`

export const docCountGQL = `
  query docCount($query: String!) {
    total: docCount(query: $query)
    trash: docCount(query: "trash:true")
    extGroups: docExtGroups {
      ext
      count
    }
  }
`

export const videoCountGQL = `
  query videoCount($query: String!) {
    total: videoCount(query: $query)
    trash: videoCount(query: "trash:true")
  }
`

export const packageCountGQL = `
  query {
    total: packageCount(query: "")
    system: packageCount(query: "type:SYSTEM")
  }
`

export const feedEntryCountGQL = `
  query {
    total: feedEntryCount(query: "")
    today: feedEntryCount(query: "today:true")
    feedsCount {
      id
      count
    }
  }
`

export const contactCountGQL = `
  query {
    total: contactCount(query: "")
  }
`

export const callCountGQL = `
  query {
    total: callCount(query: "")
    incoming: callCount(query: "type:1")
    outgoing: callCount(query: "type:2")
    missed: callCount(query: "type:3")
  }
`

export const smsCountGQL = `
  query {
    smsAllCounts {
      total
      inbox
      sent
      drafts
    }
  }
`

export const archivedConversationsGQL = `
  query {
    archivedConversations {
      ...MessageConversationFragment
    }
  }
  ${messageConversationFragment}
`

export const archivedConversationsWithAddressesGQL = `
  query {
    archivedConversations {
      ...MessageConversationWithAddressesFragment
    }
  }
  ${messageConversationWithAddressesFragment}
`

export const noteCountGQL = `
  query {
    total: noteCount(query: "")
    trash: noteCount(query: "trash:true")
  }
`

export const packagesGQL = `
  query packages($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    packages(offset: $offset, limit: $limit, query: $query, sortBy: $sortBy) {
      ...PackageFragment
    }
    packageCount(query: $query)
  }
  ${packageFragment}
`

export const packageStatusesGQL = `
  query packageStatuses($ids: [ID!]!) {
    packageStatuses(ids: $ids) {
      id
      exist
      updatedAt
    }
  }
`

export const screenMirrorStateGQL = `
  query {
    screenMirrorState
    screenMirrorControlEnabled
    screenMirrorQuality {
      mode
      resolution
    }
  }
`

export const screenMirrorVideoCodecGQL = `
  query {
    screenMirrorVideoCodec {
      annexB
      keyFrame
    }
  }
`

export const screenMirrorControlEnabledGQL = `
  query {
    screenMirrorControlEnabled
  }
`

export const screenMirrorQualityGQL = `
  query {
    screenMirrorQuality {
      mode
    }
  }
`

export const requestScreenMirrorKeyFrameGQL = `
  mutation {
    requestScreenMirrorKeyFrame
  }
`

export const notificationsGQL = `
  query {
    notifications {
      ...NotificationFragment
    }
  }
  ${notificationFragment}
`

export const deviceInfoGQL = `
  query {
    deviceInfo {
      ...DeviceInfoFragment
    }
    sims {
      id
      label
      number
      subscriptionId
    }
    battery {
      level
      voltage
      health
      plugged
      temperature
      status
      technology
      capacity
    }
  }
  ${deviceInfoFragment}
`

export const appLogsGQL = `
  query AppLogs($offset: Int!, $limit: Int!) {
    appLogs(offset: $offset, limit: $limit)
  }
`

export const appLogPathGQL = `
  query {
    appLogPath
  }
`

export const dbPathGQL = `
  query {
    dbPath
  }
`

export const dataStorePathGQL = `
  query {
    dataStorePath
  }
`

export const uploadedChunksGQL = `
  query uploadedChunks($fileId: String!) {
    uploadedChunks(fileId: $fileId)
  }
`

export const pomodoroSettingsGQL = `
  query {
    pomodoroSettings {
      workDuration
      shortBreakDuration
      longBreakDuration
      pomodorosBeforeLongBreak
      showNotification
      playSoundOnComplete
    }
  }
`

export const pomodoroTodayAndSettingsGQL = `
  query {
    pomodoroToday {
      date
      completedCount
      currentRound
      timeLeft
      totalTime
      isRunning
      isPause
      state
    }
    pomodoroSettings {
      workDuration
      shortBreakDuration
      longBreakDuration
      pomodorosBeforeLongBreak
      showNotification
      playSoundOnComplete
    }
  }
`

export const dataStoreEntriesGQL = `
  query {
    dataStoreEntries {
      key
      value
    }
  }
`

export const dbTablesGQL = `
  query {
    dbTables
  }
`

export const dbTableRowCountGQL = `
  query DbTableRowCount($table: String!) {
    dbTableRowCount(table: $table)
  }
`

export const dbTableRowsGQL = `
  query DbTableRows($table: String!, $offset: Int!, $limit: Int!) {
    dbTableRows(table: $table, offset: $offset, limit: $limit)
  }
`

export const dbTableInfoGQL = `
  query DbTableInfo($table: String!) {
    dbTableInfo(table: $table) {
      idKey
    }
  }
`

export const bookmarksGQL = `
  query {
    bookmarks {
      ...BookmarkFragment
    }
    bookmarkGroups {
      ...BookmarkGroupFragment
    }
  }
  ${bookmarkFragment}
  ${bookmarkGroupFragment}
`

export const isDiscoveringGQL = `
  query {
    isDiscovering
  }
`
