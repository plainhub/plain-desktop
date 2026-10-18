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
  clipboardItemFragment,
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
      if (e instanceof GqlError && e.cancelled) return
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
      if (e instanceof GqlError && e.cancelled) return
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
    chatItems(target: $target, query: "", offset: 0, limit: 200) {
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

export const chatConversationsGQL = `
  query {
    chatConversations {
      id
      chat { ...ChatItemFragment }
    }
  }
  ${chatItemFragment}
`

export const appFilesGQL = `
  query appFiles($offset: Int!, $limit: Int!) {
    appFiles(query: "", offset: $offset, limit: $limit) {
      id
      size
      mimeType
      fileName
      createdAt
      updatedAt
    }
    appFileCount(query: "")
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
    sms(query: $query, offset: $offset, limit: $limit) {
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
    smsConversations(query: $query, offset: $offset, limit: $limit) {
      ...SmsConversationFragment
    }
    smsConversationCount(query: $query)
  }
  ${smsConversationFragment}
`

export const smsConversationsWithAddressesGQL = `
  query smsConversations($offset: Int!, $limit: Int!, $query: String!) {
    smsConversations(query: $query, offset: $offset, limit: $limit) {
      ...SmsConversationWithAddressesFragment
    }
    smsConversationCount(query: $query)
  }
  ${smsConversationWithAddressesFragment}
`

export const contactsGQL = `
  query contacts($offset: Int!, $limit: Int!, $query: String!) {
    contacts(query: $query, offset: $offset, limit: $limit) {
      ...ContactFragment
    }
    contactCount(query: $query)
  }
  ${contactFragment}
`

export { homeStatsGQL, type HomeStatKey } from './home-stats'

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
    calls(query: $query, offset: $offset, limit: $limit) {
      ...CallFragment
    }
    callCount(query: $query)
  }
  ${callFragment}
`

export const imagesGQL = `
  query images($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    images(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
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
      error
    }
  }
`

export const videosGQL = `
  query videos($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    videos(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...VideoFragment
    }
    videoCount(query: $query)
  }
  ${videoFragment}
`

export const audiosGQL = `
  query audios($offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    items: audios(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
      ...AudioFragment
    }
    total: audioCount(query: $query)
  }
  ${audioFragment}
`

export const filesGQL = `
  query files($root: String!, $offset: Int!, $limit: Int!, $query: String!, $sortBy: FileSortBy!) {
    files(root: $root, query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
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

export const disksGQL = `
  query {
    disks {
      id
      name
      path
      sizeBytes
      removable
      model
    }
  }
`

export const sambaSettingsGQL = `
  query {
    sambaSettings {
      enabled
      username
      hasPassword
      shares {
        name
        sharePath
        auth
        readOnly
      }
      serviceName
      serviceActive
      serviceEnabled
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
    items: audioQueueItems(query: "", offset: $offset, limit: $limit) {
      ...AudioItemFragment
    }
    total: audioQueueItemCount
    playback: audioPlayback {
      revision
      mode
      mediaId
      path
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
    notes(query: $query, offset: $offset, limit: $limit) {
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
    items: feedEntries(query: $query, offset: $offset, limit: $limit) {
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
    items: docs(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
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

export const archivedSmsConversationsGQL = `
  query {
    archivedSmsConversations(query: "", offset: 0, limit: 200) {
      ...SmsConversationFragment
    }
  }
  ${smsConversationFragment}
`

export const archivedSmsConversationsWithAddressesGQL = `
  query {
    archivedSmsConversations(query: "", offset: 0, limit: 200) {
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
    packages(query: $query, offset: $offset, limit: $limit, sortBy: $sortBy) {
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

export const isScreenMirroringGQL = `
  query {
    isScreenMirroring
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
    notifications(query: "", offset: 0, limit: 200) {
      ...NotificationFragment
    }
  }
  ${notificationFragment}
`

export const clipboardItemsGQL = `
  query clipboardItems($offset: Int!, $limit: Int!, $query: String!) {
    clipboardItems(query: $query, offset: $offset, limit: $limit) {
      ...ClipboardItemFragment
    }
    clipboardItemCount(query: $query)
  }
  ${clipboardItemFragment}
`

export const deviceInfoGQL = `
  query {
    deviceInfo {
      ...DeviceInfoFragment
    }
    deviceStatus {
      ...DeviceStatusFragment
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
    appLogs(query: "", offset: $offset, limit: $limit)
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

export const uploadedChunksGQL = `
  query uploadedChunks($fileId: String!) {
    uploadedChunks(fileId: $fileId)
  }
`

export const mergeStatusGQL = `
  query mergeStatus($fileId: String!) {
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

export const userPrefsGQL = `
  query {
    userPrefs
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

export const dbTableColumnsGQL = `
  query DbTableColumns($table: String!) {
    dbTableColumns(table: $table) {
      name
      dataType
      notNull
      defaultValue
      primaryKey
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

export const appUpdateGQL = `
  query appUpdate {
    appUpdate { currentVersion latestVersion hasUpdate url releaseName publishedAt }
  }
`

export const nearbyDevicesGQL = `
  query NearbyDevices {
    nearbyDevices { id name ips bestIp port deviceType version platform lastSeen status discoveryMethods }
    pairingStates { deviceId generation mode phase error }
  }
`
