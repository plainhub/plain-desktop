import { ref } from 'vue'
import { gqlFetch, GqlError } from './gql-client'
import {
  chatItemFragment,
  chatChannelFragment,
  feedEntryFragment,
  feedFragment,
  fileFragment,
  noteFragment,
  playlistAudioFragment,
  tagFragment,
  bookmarkFragment,
  bookmarkGroupFragment,
  contactFragment,
} from './fragments'
import emitter from '@/plugins/eventbus'

// --- Mutation Wrapper ---

export interface InitMutationParams {
  document: string
}

export function initMutation(params: InitMutationParams, handleError = true) {
  const loading = ref(false)
  const doneCallbacks: Array<(result: any) => void> = []
  const errorCallbacks: Array<(error: any) => void> = []

  async function mutate(variables?: any): Promise<any> {
    loading.value = true
    try {
      const r = await gqlFetch(params.document, variables, { dedupe: false })
      if (r.errors?.length) {
        const message = r.errors[0].message
        if (handleError) emitter.emit('toast', message)
        const err = new GqlError(message)
        for (const cb of errorCallbacks) cb(err)
        return undefined
      }
      for (const cb of doneCallbacks) cb(r)
      return r
    } catch (e: any) {
      const message = e instanceof GqlError ? e.message : 'network_error'
      if (handleError) emitter.emit('toast', message)
      for (const cb of errorCallbacks) cb(e)
      return undefined
    } finally {
      loading.value = false
    }
  }

  function onDone(fn: (result: any) => void) {
    doneCallbacks.push(fn)
    return { off: () => { const i = doneCallbacks.indexOf(fn); if (i >= 0) doneCallbacks.splice(i, 1) } }
  }

  function onError(fn: (error: any) => void) {
    errorCallbacks.push(fn)
    return { off: () => { const i = errorCallbacks.indexOf(fn); if (i >= 0) errorCallbacks.splice(i, 1) } }
  }

  return { mutate, loading, onDone, onError }
}

/**
 * Wrap mutate into a Promise<boolean>.
 * Returns true on success, false on error.
 */
export async function runMutation(
  mutate: (variables?: any) => Promise<any>,
  variables?: any,
): Promise<boolean> {
  const result = await mutate(variables)
  return result != null
}

export const clearAppLogsGQL = `
  mutation {
    clearAppLogs
  }
`

export const updateDeviceNameGQL = `
  mutation updateDeviceName($name: String!) {
    updateDeviceName(name: $name)
  }
`

// --- GraphQL Mutation Definitions ---

export const sendChatItemGQL = `
  mutation sendChatItem($toId: String!, $content: String!) {
    sendChatItem(toId: $toId, content: $content) {
      ...ChatItemFragment
    }
  }
  ${chatItemFragment}
`

export const deleteChatItemGQL = `
  mutation deleteChatItem($id: ID!) {
    deleteChatItem(id: $id)
  }
`

export const deleteChatItemsGQL = `
  mutation deleteChatItems($query: String!) {
    deleteChatItems(query: $query)
  }
`

export const retryChatItemGQL = `
  mutation retryChatItem($id: ID!) {
    retryChatItem(id: $id) {
      ...ChatItemFragment
    }
  }
  ${chatItemFragment}
`
export const createChatChannelGQL = `
  mutation createChatChannel($name: String!) {
    createChatChannel(name: $name) {
      ...ChatChannelFragment
    }
  }
  ${chatChannelFragment}
`

export const updateChatChannelGQL = `
  mutation updateChatChannel($id: ID!, $name: String!) {
    updateChatChannel(id: $id, name: $name) {
      ...ChatChannelFragment
    }
  }
  ${chatChannelFragment}
`

export const deleteChatChannelGQL = `
  mutation deleteChatChannel($id: ID!) {
    deleteChatChannel(id: $id)
  }
`

export const deletePeerGQL = `
  mutation deletePeer($id: ID!) {
    deletePeer(id: $id)
  }
`

export const unpairPeerGQL = `
  mutation unpairPeer($id: ID!) {
    unpairPeer(id: $id)
  }
`

export const leaveChatChannelGQL = `
  mutation leaveChatChannel($id: ID!) {
    leaveChatChannel(id: $id)
  }
`

export const addChatChannelMemberGQL = `
  mutation addChatChannelMember($id: ID!, $peerId: String!) {
    addChatChannelMember(id: $id, peerId: $peerId) {
      ...ChatChannelFragment
    }
  }
  ${chatChannelFragment}
`

export const removeChatChannelMemberGQL = `
  mutation removeChatChannelMember($id: ID!, $peerId: String!) {
    removeChatChannelMember(id: $id, peerId: $peerId) {
      ...ChatChannelFragment
    }
  }
  ${chatChannelFragment}
`

export const acceptChatChannelInviteGQL = `
  mutation acceptChatChannelInvite($id: ID!) {
    acceptChatChannelInvite(id: $id)
  }
`

export const respondChannelInviteGQL = `
  mutation respondChannelInvite($id: ID!, $accept: Boolean!) {
    respondChannelInvite(id: $id, accept: $accept)
  }
`

export const declineChatChannelInviteGQL = `
  mutation declineChatChannelInvite($id: ID!) {
    declineChatChannelInvite(id: $id)
  }
`

export const createDirGQL = `
  mutation createDir($path: String!) {
    createDir(path: $path) {
      ...FileFragment
    }
  }
  ${fileFragment}
`

export const writeTextFileGQL = `
  mutation writeTextFile($path: String!, $content: String!, $overwrite: Boolean!) {
    writeTextFile(path: $path, content: $content, overwrite: $overwrite) {
      ...FileFragment
    }
  }
  ${fileFragment}
`

export const renameFileGQL = `
  mutation renameFile($path: String!, $name: String!) {
    renameFile(path: $path, name: $name)
  }
`

export const copyFileGQL = `
  mutation copyFile($src: String!, $dst: String!, $overwrite: Boolean!) {
    copyFile(src: $src, dst: $dst, overwrite: $overwrite)
  }
`

export const moveFileGQL = `
  mutation moveFile($src: String!, $dst: String!, $overwrite: Boolean!) {
    moveFile(src: $src, dst: $dst, overwrite: $overwrite)
  }
`

export const playAudioGQL = `
  mutation playAudio($path: String!) {
    playAudio(path: $path) {
      ...PlaylistAudioFragment
    }
  }
  ${playlistAudioFragment}
`

export const updateAudioPlayModeGQL = `
  mutation updateAudioPlayMode($mode: MediaPlayMode!) {
    updateAudioPlayMode(mode: $mode)
  }
`

export const deletePlaylistAudioGQL = `
  mutation deletePlaylistAudio($path: String!) {
    deletePlaylistAudio(path: $path)
  }
`

export const addPlaylistAudiosGQL = `
  mutation addPlaylistAudios($query: String!) {
    addPlaylistAudios(query: $query)
  }
`

export const clearAudioPlaylistGQL = `
  mutation clearAudioPlaylist {
    clearAudioPlaylist
  }
`

export const reorderPlaylistAudiosGQL = `
  mutation reorderPlaylistAudios($paths: [String!]!) {
    reorderPlaylistAudios(paths: $paths)
  }
`

export const deleteMediaItemsGQL = `
  mutation deleteMediaItems($type: DataType!, $query: String!) {
    deleteMediaItems(type: $type, query: $query) {
      type
      query
    }
  }
`

export const trashMediaItemsGQL = `
  mutation trashMediaItems($type: DataType!, $query: String!) {
    trashMediaItems(type: $type, query: $query) {
      type
      query
    }
  }
`

export const restoreMediaItemsGQL = `
  mutation restoreMediaItems($type: DataType!, $query: String!) {
    restoreMediaItems(type: $type, query: $query) {
      type
      query
    }
  }
`

export const moveMediaItemsGQL = `
  mutation moveMediaItems($type: DataType!, $query: String!, $destDir: String!) {
    moveMediaItems(type: $type, query: $query, destDir: $destDir) {
      type
      query
    }
  }
`

export const removeFromTagsGQL = `
  mutation removeFromTags($type: DataType!, $tagIds: [ID!]!, $query: String!) {
    removeFromTags(type: $type, tagIds: $tagIds, query: $query)
  }
`

export const addToTagsGQL = `
  mutation addToTags($type: DataType!, $tagIds: [ID!]!, $query: String!) {
    addToTags(type: $type, tagIds: $tagIds, query: $query)
  }
`

export const updateTagRelationsGQL = `
  mutation updateTagRelations($type: DataType!, $item: TagRelationStub!, $addTagIds: [ID!]!, $removeTagIds: [ID!]!) {
    updateTagRelations(type: $type, item: $item, addTagIds: $addTagIds, removeTagIds: $removeTagIds)
  }
`

export const createTagGQL = `
  mutation createTag($type: DataType!, $name: String!) {
    createTag(type: $type, name: $name) {
      ...TagFragment
    }
  }
  ${tagFragment}
`

export const updateTagGQL = `
  mutation updateTag($id: ID!, $name: String!) {
    updateTag(id: $id, name: $name) {
      ...TagFragment
    }
  }
  ${tagFragment}
`

export const deleteTagGQL = `
  mutation deleteTag($id: ID!) {
    deleteTag(id: $id)
  }
`

export const addFavoriteFolderGQL = `
  mutation addFavoriteFolder($rootPath: String!, $fullPath: String!) {
    addFavoriteFolder(rootPath: $rootPath, fullPath: $fullPath) {
      rootPath
      fullPath
    }
  }
`

export const removeFavoriteFolderGQL = `
  mutation removeFavoriteFolder($fullPath: String!) {
    removeFavoriteFolder(fullPath: $fullPath) {
      rootPath
      fullPath
      alias
    }
  }
`

export const setFavoriteFolderAliasGQL = `
  mutation setFavoriteFolderAlias($fullPath: String!, $alias: String!) {
    setFavoriteFolderAlias(fullPath: $fullPath, alias: $alias) {
      rootPath
      fullPath
      alias
    }
  }
`

export const saveNoteGQL = `
  mutation saveNote($id: ID!, $input: NoteInput!) {
    saveNote(id: $id, input: $input) {
      ...NoteFragment
    }
  }
  ${noteFragment}
`

export const deleteNotesGQL = `
  mutation deleteNotes($query: String!) {
    deleteNotes(query: $query)
  }
`

export const trashNotesGQL = `
  mutation trashNotes($query: String!) {
    trashNotes(query: $query)
  }
`

export const restoreNotesGQL = `
  mutation restoreNotes($query: String!) {
    restoreNotes(query: $query)
  }
`

export const deleteFeedEntriesGQL = `
  mutation deleteFeedEntries($query: String!) {
    deleteFeedEntries(query: $query)
  }
`

export const deleteCallsGQL = `
  mutation deleteCalls($query: String!) {
    deleteCalls(query: $query)
  }
`

export const deleteContactsGQL = `
  mutation deleteContacts($query: String!) {
    deleteContacts(query: $query)
  }
`

export const createFeedGQL = `
  mutation createFeed($url: String!, $fetchContent: Boolean!) {
    createFeed(url: $url, fetchContent: $fetchContent) {
      ...FeedFragment
    }
  }
  ${feedFragment}
`

export const importFeedsGQL = `
  mutation importFeeds($content: String!) {
    importFeeds(content: $content)
  }
`

export const exportFeedsGQL = `
  mutation exportFeeds {
    exportFeeds
  }
`

export const exportNotesGQL = `
  mutation exportNotes($query: String!) {
    exportNotes(query: $query)
  }
`

export const relaunchAppGQL = `
  mutation relaunchApp {
    relaunchApp
  }
`

export const openAccessibilitySettingsGQL = `
  mutation openAccessibilitySettings {
    openAccessibilitySettings
  }
`

export const openWebSettingsGQL = `
  mutation openWebSettings {
    openWebSettings
  }
`

export const deleteFeedGQL = `
  mutation deleteFeed($id: ID!) {
    deleteFeed(id: $id)
  }
`

export const updateFeedGQL = `
  mutation updateFeed($id: ID!, $name: String!, $fetchContent: Boolean!) {
    updateFeed(id: $id, name: $name, fetchContent: $fetchContent) {
      ...FeedFragment
    }
  }
  ${feedFragment}
`

export const syncFeedsGQL = `
  mutation syncFeeds($id: ID) {
    syncFeeds(id: $id)
  }
`

export const syncFeedContentGQL = `
  mutation syncFeedContent($id: ID!) {
    syncFeedContent(id: $id) {
      ...FeedEntryFragment
      feed {
        ...FeedFragment
      }
    }
  }
  ${feedFragment}
  ${feedEntryFragment}
`

export const callGQL = `
  mutation call($number: String!, $showDialer: Boolean!) {
    call(number: $number, showDialer: $showDialer)
  }
`

export const setClipGQL = `
  mutation setClip($text: String!) {
    setClip(text: $text)
  }
`

export const sendSmsGQL = `
  mutation sendSms($number: String!, $body: String!, $subscriptionId: Int!) {
    sendSms(number: $number, body: $body, subscriptionId: $subscriptionId)
  }
`

export const sendSmsWithRequestIdGQL = `
  mutation sendSms($number: String!, $body: String!, $subscriptionId: Int!, $requestId: String!) {
    sendSms(number: $number, body: $body, subscriptionId: $subscriptionId, requestId: $requestId)
  }
`

export const archiveConversationGQL = `
  mutation archiveConversation($id: String!, $date: Long!) {
    archiveConversation(id: $id, date: $date)
  }
`

export const unarchiveConversationGQL = `
  mutation unarchiveConversation($id: String!) {
    unarchiveConversation(id: $id)
  }
`

export const sendMmsGQL = `
  mutation sendMms($number: String!, $body: String!, $attachmentPaths: [String!]!, $threadId: String!) {
    sendMms(number: $number, body: $body, attachmentPaths: $attachmentPaths, threadId: $threadId)
  }
`

export const uninstallPackagesGQL = `
  mutation uninstallPackages($ids: [ID!]!) {
    uninstallPackages(ids: $ids)
  }
`

export const uninstallPackageGQL = `
  mutation uninstallPackages($id: ID!) {
    uninstallPackages(ids: [$id])
  }
`

export const installPackageGQL = `
  mutation installPackage($path: String!) {
    installPackage(path: $path) {
      packageName
      updatedAt
      isNew
    }
  }
`

export const startScreenMirrorGQL = `
  mutation startScreenMirror($audio: Boolean!) {
    startScreenMirror(audio: $audio)
  }
`

export const requestScreenMirrorAudioGQL = `
  mutation requestScreenMirrorAudio {
    requestScreenMirrorAudio
  }
`

export const stopScreenMirrorGQL = `
  mutation stopScreenMirror {
    stopScreenMirror
  }
`

export const setTempValueGQL = `
  mutation setTempValue($key: String!, $value: String!) {
    setTempValue(key: $key, value: $value) {
      key
      value
    }
  }
`

export const cancelNotificationsGQL = `
  mutation cancelNotifications($ids: [ID!]!) {
    cancelNotifications(ids: $ids)
  }
`

export const replyNotificationGQL = `
  mutation replyNotification($id: ID!, $actionIndex: Int!, $text: String!) {
    replyNotification(id: $id, actionIndex: $actionIndex, text: $text)
  }
`

export const updateScreenMirrorQualityGQL = `
  mutation updateScreenMirrorQuality($mode: ScreenMirrorMode!) {
    updateScreenMirrorQuality(mode: $mode)
  }
`

export const saveFeedEntriesToNotesGQL = `
  mutation saveFeedEntriesToNotes($query: String!) {
    saveFeedEntriesToNotes(query: $query)
  }
`

export const mergeChunksGQL = `
  mutation mergeChunks($fileId: String!, $totalChunks: Int!, $path: String!, $replace: Boolean!, $isAppFile: Boolean!, $totalSize: Long!) {
    mergeChunks(fileId: $fileId, totalChunks: $totalChunks, path: $path, replace: $replace, isAppFile: $isAppFile, totalSize: $totalSize)
  }
`

export const mergeChunksAsyncGQL = `
  mutation mergeChunksAsync($fileId: String!, $totalChunks: Int!, $path: String!, $replace: Boolean!, $isAppFile: Boolean!, $totalSize: Long!) {
    mergeChunksAsync(fileId: $fileId, totalChunks: $totalChunks, path: $path, replace: $replace, isAppFile: $isAppFile, totalSize: $totalSize)
  }
`

export const deleteChunksGQL = `
  mutation deleteChunks($fileId: String!) {
    deleteChunks(fileId: $fileId)
  }
`

export const startPomodoroGQL = `
  mutation startPomodoro($timeLeft: Int!) {
    startPomodoro(timeLeft: $timeLeft)
  }
`

export const stopPomodoroGQL = `
  mutation stopPomodoro {
    stopPomodoro
  }
`

export const pausePomodoroGQL = `
  mutation pausePomodoro {
    pausePomodoro
  }
`

export const sendScreenMirrorControlGQL = `
  mutation sendScreenMirrorControl($input: ScreenMirrorControlInput!) {
    sendScreenMirrorControl(input: $input)
  }
`

export const addBookmarksGQL = `
  mutation addBookmarks($urls: [String!]!, $groupId: String!) {
    addBookmarks(urls: $urls, groupId: $groupId) {
      ...BookmarkFragment
    }
  }
  ${bookmarkFragment}
`

export const updateBookmarkGQL = `
  mutation updateBookmark($id: ID!, $input: BookmarkInput!) {
    updateBookmark(id: $id, input: $input) {
      ...BookmarkFragment
    }
  }
  ${bookmarkFragment}
`

export const deleteBookmarksGQL = `
  mutation deleteBookmarks($ids: [ID!]!) {
    deleteBookmarks(ids: $ids)
  }
`

export const recordBookmarkClickGQL = `
  mutation recordBookmarkClick($id: ID!) {
    recordBookmarkClick(id: $id)
  }
`

export const createBookmarkGroupGQL = `
  mutation createBookmarkGroup($name: String!) {
    createBookmarkGroup(name: $name) {
      ...BookmarkGroupFragment
    }
  }
  ${bookmarkGroupFragment}
`

export const updateBookmarkGroupGQL = `
  mutation updateBookmarkGroup($id: ID!, $name: String!, $collapsed: Boolean!, $sortOrder: Int!) {
    updateBookmarkGroup(id: $id, name: $name, collapsed: $collapsed, sortOrder: $sortOrder) {
      ...BookmarkGroupFragment
    }
  }
  ${bookmarkGroupFragment}
`

export const deleteBookmarkGroupGQL = `
  mutation deleteBookmarkGroup($id: ID!) {
    deleteBookmarkGroup(id: $id)
  }
`

export const deleteFilesGQL = `
  mutation deleteFiles($paths: [String!]!) {
    deleteFiles(paths: $paths)
  }
`

export const enableImageSearchGQL = `
  mutation { enableImageSearch }
`

export const disableImageSearchGQL = `
  mutation { disableImageSearch }
`

export const cancelImageModelDownloadGQL = `
  mutation { cancelImageModelDownload }
`

export const startImageIndexGQL = `
  mutation startImageIndex($force: Boolean) {
    startImageIndex(force: $force)
  }
`

export const cancelImageIndexGQL = `
  mutation { cancelImageIndex }
`

export const createContactGQL = `
  mutation createContact($input: ContactInput!) {
    createContact(input: $input) {
      ...ContactFragment
    }
  }
  ${contactFragment}
`

export const updateContactGQL = `
  mutation updateContact($id: ID!, $input: ContactInput!) {
    updateContact(id: $id, input: $input) {
      ...ContactFragment
    }
  }
  ${contactFragment}
`

export const deleteContactGQL = `
  mutation DeleteContact($query: String!) {
    deleteContacts(query: $query)
  }
`

export const deleteCallGQL = `
  mutation DeleteCall($query: String!) {
    deleteCalls(query: $query)
  }
`

export const deleteNoteGQL = `
  mutation DeleteNote($query: String!) {
    deleteNotes(query: $query)
  }
`

export const deleteFeedEntryGQL = `
  mutation deleteFeedEntry($query: String!) {
    deleteFeedEntries(query: $query)
  }
`

export const deleteDataStoreEntryGQL = `
  mutation DeleteDataStoreEntry($key: String!) {
    deleteDataStoreEntry(key: $key)
  }
`

export const createDbTableRowGQL = `
  mutation CreateDbTableRow($table: String!, $row: String!) {
    createDbTableRow(table: $table, row: $row)
  }
`

export const deleteDbTableRowsGQL = `
  mutation DeleteDbTableRows($table: String!, $ids: [String!]!) {
    deleteDbTableRows(table: $table, ids: $ids)
  }
`

export const startDiscoveryGQL = `
  mutation {
    startDiscovery
  }
`

export const stopDiscoveryGQL = `
  mutation {
    stopDiscovery
  }
`

export const pairDeviceGQL = `
  mutation pairDevice($input: PairingDeviceInput!) {
    pairDevice(input: $input)
  }
`

export const cancelPairingGQL = `
  mutation cancelPairing($deviceId: String!) {
    cancelPairing(deviceId: $deviceId)
  }
`

export const respondToPairingGQL = `
  mutation respondToPairing($input: PairingRequestInput!, $accepted: Boolean!) {
    respondToPairing(input: $input, accepted: $accepted)
  }
`

export const downloadPeerFileGQL = `
  mutation downloadPeerFile($messageId: ID!, $peerId: ID!) {
    downloadPeerFile(messageId: $messageId, peerId: $peerId)
  }
`

export const pauseDownloadGQL = `
  mutation pauseDownload($messageId: ID!) {
    pauseDownload(messageId: $messageId)
  }
`

export const resumeDownloadGQL = `
  mutation resumeDownload($messageId: ID!, $peerId: ID!) {
    resumeDownload(messageId: $messageId, peerId: $peerId)
  }
`

export const retryDownloadGQL = `
  mutation retryDownload($messageId: ID!, $peerId: ID!) {
    retryDownload(messageId: $messageId, peerId: $peerId)
  }
`
