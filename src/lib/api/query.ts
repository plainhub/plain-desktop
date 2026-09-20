import { ref, watch, nextTick, getCurrentInstance, onActivated, onDeactivated, type Ref } from 'vue'
import { gqlFetch, GqlError } from './gql-client'
import {
  chatItemFragment,
  chatChannelFragment,
  smsFragment,
  smsConversationFragment,
  smsConversationWithAddressesFragment,
  contactFragment,
  callFragment,
  imageFragment,
  videoFragment,
  audioFragment,
  fileFragment,
  appFragment,
  audioItemFragment,
  tagFragment,
  noteFragment,
  feedFragment,
  feedEntryFragment,
  packageFragment,
  tagSubFragment,
  notificationFragment,
  clipboardFragment,
  deviceInfoFragment,
  deviceStatusFragment,
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
  /** Static SDL, or a getter re-evaluated on change (e.g. feature-dependent
   *  queries that must wait for the device type to be known). */
  document: string | (() => string)
  variables?: any
  /** When provided and returns false the query is never sent (e.g. phone-only
   *  features on a NAS). Re-checked reactively: fired once it turns true. */
  enabled?: () => boolean
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
    if (params.enabled && !params.enabled()) return
    loading.value = true
    try {
      const v = vars ?? resolveVars(params.variables)
      const document = typeof params.document === 'function' ? params.document() : params.document
      const r = await gqlFetch<TResult>(document, v)
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

  // Guard watcher so deactivated keep-alive instances don't fire queries
  // when the shared reactive route changes.
  // nextTick defers execution until after KeepAlive lifecycle hooks complete.
  let active = true
  const inst = getCurrentInstance()
  if (inst) {
    onDeactivated(() => { active = false })
    onActivated(() => { active = true })
  }
  if (typeof params.variables === 'function') {
    watch(params.variables, async () => {
      await nextTick()
      if (active) execute()
    }, { deep: true })
  }
  if (typeof params.document === 'function') {
    watch(params.document, async () => {
      await nextTick()
      if (active) execute()
    })
  }
  if (params.enabled) {
    watch(params.enabled, async (now, was) => {
      await nextTick()
      if (active && now && !was) execute()
    })
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
      const document = typeof params.document === 'function' ? params.document() : params.document
      const r = options.force
        ? await gqlFetch<TResult>(document, v, { fresh: true })
        : await gqlFetch<TResult>(document, v)
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
  query ($target: String!) {
    chatItems(target: $target, offset: 0, limit: 200, query: "") {
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
    appFiles(offset: $offset, limit: $limit, query: "") {
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

export const tagRelationsGQL = `
  query ($type: DataType!, $keys: [String!]!) {
    tagRelations(type: $type, keys: $keys) {
      tagId
      key
    }
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
  query ($path: String!, $fileName: String) {
    fileInfo(path: $path, fileName: $fileName) {
      ... on FileInfo {
        path
        updatedAt
        size
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
          durationMs
          width
          height
          location {
            latitude
            longitude
          }
        }
        ... on AudioFileInfo {
          durationMs
          location {
            latitude
            longitude
          }
        }
      }
    }
  }
`

export const smsGQL = `
  query sms($offset: Int!, $limit: Int!, $query: String!) {
    sms(offset: $offset, limit: $limit, query: $query) {
      ...SmsFragment
    }
    smsCount(query: $query)
  }
  ${smsFragment}
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
      ...SmsConversationFragment
    }
    smsConversationCount(query: $query)
  }
  ${smsConversationFragment}
`

export const smsConversationsWithAddressesGQL = `
  query smsConversations($offset: Int!, $limit: Int!, $query: String!) {
    smsConversations(offset: $offset, limit: $limit, query: $query) {
      ...SmsConversationWithAddressesFragment
    }
    smsConversationCount(query: $query)
  }
  ${smsConversationWithAddressesFragment}
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

/** Count fields keyed by the home card count key. Servers differ: plain-nas
 *  only implements audio/image/video counts, the phone has the rest. */
export type HomeStatKey =
  | 'audios'
  | 'images'
  | 'videos'
  | 'docs'
  | 'packages'
  | 'notes'
  | 'feedEntries'
  | 'messages'
  | 'calls'
  | 'contacts'

const HOME_STATS_COUNT_FIELDS: Record<HomeStatKey, string> = {
  audios: 'audioCount(query: $mediaQuery)',
  images: 'imageCount(query: $mediaQuery)',
  videos: 'videoCount(query: $mediaQuery)',
  docs: 'docCount(query: "")',
  packages: 'packageCount(query: "")',
  notes: 'noteCount(query: "")',
  feedEntries: 'feedEntryCount(query: "")',
  messages: 'smsCount(query: "")',
  calls: 'callCount(query: "")',
  contacts: 'contactCount(query: "")',
}

/** Build the home stats query requesting only `countKeys` (plus mounts).
 *  Without keys this is a mounts-only probe, useful before the device type
 *  is known. */
export function homeStatsGQL(countKeys: readonly HomeStatKey[] = []): string {
  const fields = countKeys.map((key) => HOME_STATS_COUNT_FIELDS[key]).join('\n    ')
  return `
  query homeStats($mediaQuery: String!) {
    ${fields}
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
}

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

export const scanProgressGQL = `
  query {
    scanProgress {
      indexed
      pending
      total
      state
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
      diskId
    }
  }
`

export const favoriteFoldersGQL = `
  query {
    favoriteFolders {
      rootPath
      fullPath
      alias
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

export const audioQueueGQL = `
  query audioQueue($offset: Int!, $limit: Int!) {
    items: audioQueueItems(offset: $offset, limit: $limit, query: "") {
      ...AudioItemFragment
    }
    total: audioQueueItemCount
    playback: audioPlayback {
      mode
      currentPath
      isPlaying
      positionMs
    }
  }
  ${audioItemFragment}
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
  query mediaBuckets($type: MediaDataType!) {
    mediaBuckets(type: $type) {
      id
      name
      itemCount
      topItemPaths
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
  query bucketsTags($type: MediaDataType!, $tagType: DataType!) {
    tags(type: $tagType) {
      ...TagFragment
    }
    mediaBuckets(type: $type) {
      id
      name
      itemCount
      topItemPaths
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
    feedEntryCounts {
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
    smsBoxCounts {
      total
      inbox
      sent
      drafts
    }
  }
`

export const archivedConversationsGQL = `
  query {
    archivedConversations(offset: 0, limit: 200, query: "") {
      ...SmsConversationFragment
    }
  }
  ${smsConversationFragment}
`

export const archivedConversationsWithAddressesGQL = `
  query {
    archivedConversations(offset: 0, limit: 200, query: "") {
      ...SmsConversationWithAddressesFragment
    }
  }
  ${smsConversationWithAddressesFragment}
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
      exists
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
    notifications(offset: 0, limit: 200, query: "") {
      ...NotificationFragment
    }
  }
  ${notificationFragment}
`

export const clipboardGQL = `
  query clipboard($offset: Int!, $limit: Int!, $query: String!) {
    clipboard(offset: $offset, limit: $limit, query: $query) {
      ...ClipboardFragment
    }
    clipboardCount(query: $query)
  }
  ${clipboardFragment}
`

export const deviceInfoGQL = `
  query {
    deviceInfo {
      ...DeviceInfoFragment
    }
    deviceStatus {
      ...DeviceStatusFragment
    }
    sims {
      id
      label
      number
      subscriptionId
    }
  }
  ${deviceInfoFragment}
  ${deviceStatusFragment}
`

export const deviceStatusGQL = `
  query {
    deviceStatus {
      batteryLevel
      charging
    }
  }
`

export const appLogsGQL = `
  query AppLogs($offset: Int!, $limit: Int!) {
    appLogs(offset: $offset, limit: $limit, query: "")
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
  query uploadedChunks($fileId: ID!) {
    uploadedChunks(fileId: $fileId)
  }
`

export const mergeStatusGQL = `
  query mergeStatus($fileId: ID!) {
    mergeStatus(fileId: $fileId) {
      status
      value
      mergedSize
      error
    }
  }
`

export const pomodoroSettingsGQL = `
  query {
    pomodoroSettings {
      workDurationMin
      shortBreakDurationMin
      longBreakDurationMin
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
      timeLeftSec
      totalTimeSec
      isRunning
      isPaused
      state
    }
    pomodoroSettings {
      workDurationMin
      shortBreakDurationMin
      longBreakDurationMin
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
