import type { IFile } from './file'
import { ChatStatus, ChannelStatus, ChannelSystemMessageAction, ChannelSystemMessageType, DeviceType, DriveType, ImageSearchStatusType, MemberStatus, PackageType, PeerStatus, AppChannelType, ScreenMirrorMode } from './status'

export interface IData {
  id: string
}

export interface ISim {
  id: string
  label: string
  number: string
  subscriptionId: number
}

export interface ITag extends IData {
  id: string
  name: string
  count: number
}

export interface IType extends IData {
  id: string
  name: string
}

export interface IPage {
  path: string // full path
  sidebar?: boolean
}

export interface IBucket extends IData {
  id: string
  name: string
  itemCount: number
  topItems: string[]
}

export interface IMessage extends IData {
  id: string
  body: string
  address: string
  serviceCenter: string
  date: string
  type: number
  threadId: string
  subscriptionId: number
  isMms?: boolean
  attachments?: IMessageAttachment[]
  tags: ITag[]
}

export interface IMessageAttachment {
  path: string
  contentType: string
  name: string
}

export interface IMessageConversation extends IData {
  id: string
  address: string
  /** Participant-aware servers expose every resolved thread address. */
  addresses?: string[]
  snippet: string
  date: string
  messageCount: number
  read: boolean
}

export interface ISmsChangedEvent {
  uris: string[]
}

export interface ISmsSendResultEvent {
  clientId?: string | null
  requestId?: string | null
  success: boolean
  resultCode?: number
}

export interface IMmsSendResultEvent {
  pendingId: string
  success: boolean
  resultCode?: number
}

export interface IContactContentItem {
  label: string
  value: string
  type: number
}

export interface IContactPhoneNumber extends IContactContentItem {
  normalizedNumber: string
}

export interface IContactSource {
  name: string
  type: number
}

export interface IPackageStatus {
  id: string
  exist: boolean
  updatedAt: string
}

export interface IContact extends IData {
  id: string
  suffix: string
  prefix: string
  firstName: string
  middleName: string
  lastName: string
  updatedAt: string
  notes: string
  source: string
  thumbnailId: string
  starred: boolean
  phoneNumbers: IContactPhoneNumber[]
  addresses: IContactContentItem[]
  emails: IContactContentItem[]
  websites: IContactContentItem[]
  events: IContactContentItem[]
  ims: IContactContentItem[]
  tags: ITag[]
}

export interface ICallGeo {
  isp: string
  city: string
  province: string
}

export interface ICall extends IData {
  id: string
  name: string
  number: string
  duration: number
  accountId: string
  startedAt: string
  photoId: string
  type: number
  geo?: ICallGeo
  tags: ITag[]
}

export interface INote extends IData {
  id: string
  title: string
  content: string
  tags: ITag[]
  createdAt: string
  updatedAt: string
  deletedAt: string
}

export interface IFeedEntry extends IData {
  id: string
  title: string
  url: string
  image: string
  description: string
  content: string
  author: string
  feedId: string
  rawId: string
  tags: ITag[]
  publishedAt: string
  createdAt: string
  updatedAt: string
}

export interface IFeedEntryDetail extends IFeedEntry {
  feed?: IFeed
}

export interface IMedia extends IData {
  id: string
  title: string
  path: string
  size: number
  bucketId: string
  tags: ITag[]
  createdAt: string
  updatedAt: string
}

export interface IAudio extends IMedia {
  artist: string
  albumFileId: string
  duration: number
}

export interface IImage extends IMedia {
  takenAt?: string
}

export interface IVideo extends IMedia {
  duration: number
  takenAt?: string
}

export interface IDoc {
  id: string
  title: string
  path: string
  extension: string
  size: number
  bucketId: string
  createdAt: string
  updatedAt: string
  tags: ITag[]
}

export interface IDocExtGroup {
  ext: string
  count: number
}

export interface IPlaylistAudio {
  title: string
  artist: string
  path: string
  fileId: string
  duration: number
  size: number
}

export interface IFilter {
  tagIds: string[]
  text?: string
  bucketId?: string
  feedId?: string
  today?: boolean
  type?: string
  trash?: boolean
}

export interface IFileFilter {
  showHidden: boolean
  type: string
  rootPath: string
  text: string
  parent: string
  fileSize?: string
}

export interface IDropdownItem {
  text: string
  click: () => void
}

export interface ITagRelationStub {
  key: string
  title: string
  size: number
}

export interface IChatItem extends IData {
  id: string
  fromId: string
  toId: string
  channelId: string
  createdAt: string
  content: string
  _content: any
  __typename: string
  data: any
  status?: ChatStatus
  statusData?: string
}

export interface IPeer {
  id: string
  name: string
  ip: string
  status: PeerStatus
  online?: boolean
  port: number
  deviceType: DeviceType
  createdAt: string
  updatedAt: string
}

export interface IChatChannelMember {
  id: string
  status: MemberStatus
}

export interface IChatChannel {
  id: string
  name: string
  owner: string
  members: IChatChannelMember[]
  version: number
  status: ChannelStatus
  createdAt: string
  updatedAt: string
}

export interface IFeedCount {
  id: string
  count: number
}

export interface IImageItem extends IImage {
  fileId: string
}
export interface IVideoItem extends IVideo {
  fileId: string
}
export interface IAudioItem extends IAudio {
  fileId: string
}

export interface IFeed extends IData {
  id: string
  name: string
  url: string
  fetchContent: boolean
}

export interface INotification extends IData {
  id: string
  onlyOnce: boolean
  isClearable: boolean
  appId: string
  appName: string
  time: string
  silent: boolean
  title: string
  body: string
  icon: string
  actions: string[]
  replyActions: string[]
}

export interface IPackage extends IData {
  id: string
  name: string
  type: PackageType
  version: string
  path: string
  size: number
  icon: string
  installedAt: string
  updatedAt: string
}

export interface IPackageItem extends IPackage {
  isUninstalling: boolean
}

// deleted, trashed, restored
export interface IMediaItemsActionedEvent {
  type: string
  action: string
  query: string
  id?: string
}
// deleted, trashed, restored, saved
export interface INotesActionedEvent {
  action: string
  id?: string
  note?: INote
}

export interface IFileDeletedEvent {
  item: IFile
}

export interface IFileRenamedEvent {
  oldPath: string
  newPath: string
  item: IFile
}

export interface IItemTagsUpdatedEvent {
  item: ITagRelationStub
  type: string
}

export interface IItemsTagsUpdatedEvent {
  type: string
}

export interface IScreenMirrorQuality {
  resolution: number
  quality: number
}

export interface IScreenMirrorQualityOption {
  id: string
  data?: IScreenMirrorQuality
}

export interface IStorageMount {
  id: string
  name: string
  path: string
  mountPoint: string
  fsType: string
  totalBytes: number
  usedBytes: number
  freeBytes: number
  remote: boolean
  alias: string
  driveType: DriveType
  diskID: string
}

export interface IHomeStats {
  callCount: number
  contactCount: number
  smsCount: number
  noteCount: number
  docCount: number
  mediaCount: number
  feedEntryCount: number
  videoCount: number
  audioCount: number
  imageCount: number
  packageCount: number
  mounts: IStorageMount[]
}

export interface IFavoriteFolder {
  rootPath: string
  fullPath: string
  alias?: string | null
}

export interface IImageSearchStatus {
  status: ImageSearchStatusType
  downloadProgress: number
  errorMessage: string
  modelSize: number
  modelDir: string
  isIndexing: boolean
  totalImages: number
  indexedImages: number
}

export interface IApp {
  clientId: string
  usbConnected: boolean
  urlToken: string
  httpPort: number
  httpsPort: number
  appDir: string
  deviceName: string
  deviceType: DeviceType
  battery: number
  appVersion: string
  osVersion: number
  channel: AppChannelType
  permissions: string[]
  audios: IPlaylistAudio[]
  audioCurrent: string
  audioMode: string
  sdcardPath: string
  usbDiskPaths: string[]
  internalStoragePath: string
  downloadsDir: string
  developerMode: boolean
  debug: boolean
  favoriteFolders: IFavoriteFolder[]
}

export interface IBreadcrumbItem {
  path: string
  name: string
}
