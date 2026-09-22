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
  topItemPaths: string[]
}

export type SmsType = 'INBOX' | 'SENT' | 'DRAFT' | 'OUTBOX' | 'FAILED' | 'QUEUED' | 'UNKNOWN'

export interface ISms extends IData {
  id: string
  body: string
  address: string
  serviceCenter: string
  sentAt: string
  type: SmsType
  threadId: string
  subscriptionId: number
  isMms?: boolean
  attachments?: ISmsAttachment[]
  tags: ITag[]
}

export interface ISmsAttachment {
  path: string
  contentType: string
  name: string
}

export interface ISmsConversation extends IData {
  id: string
  address: string
  /** Participant-aware servers expose every resolved thread address. */
  addresses?: string[]
  snippet: string
  lastMessageAt: string
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

export type PhoneType =
  | 'CUSTOM' | 'HOME' | 'MOBILE' | 'WORK' | 'FAX_WORK' | 'FAX_HOME' | 'PAGER'
  | 'OTHER' | 'CALLBACK' | 'CAR' | 'COMPANY_MAIN' | 'ISDN' | 'MAIN' | 'OTHER_FAX'
  | 'RADIO' | 'TELEX' | 'TTY_TDD' | 'WORK_MOBILE' | 'WORK_PAGER' | 'ASSISTANT'

export type EmailType = 'CUSTOM' | 'HOME' | 'WORK' | 'OTHER' | 'MOBILE'

export type PostalType = 'CUSTOM' | 'HOME' | 'WORK' | 'OTHER'

export type ContactEventType = 'CUSTOM' | 'ANNIVERSARY' | 'BIRTHDAY' | 'OTHER'

export type WebsiteType = 'CUSTOM' | 'HOMEPAGE' | 'BLOG' | 'FTP' | 'HOME' | 'WORK' | 'OTHER'

export type ImProtocol =
  | 'CUSTOM' | 'AIM' | 'MSN' | 'YAHOO' | 'SKYPE' | 'QQ' | 'GOOGLE_TALK'
  | 'ICQ' | 'JABBER' | 'NETMEETING'

export interface IContactPhoneNumber {
  value: string
  type: PhoneType
  label: string
  normalizedNumber: string
}

export interface IContactEmail {
  value: string
  type: EmailType
  label: string
}

export interface IContactAddress {
  value: string
  type: PostalType
  label: string
}

export interface IContactEvent {
  value: string
  type: ContactEventType
  label: string
}

export interface IContactWebsite {
  value: string
  type: WebsiteType
  label: string
}

export interface IContactIm {
  value: string
  protocol: ImProtocol
  customProtocol: string
}

export interface IContactSource {
  name: string
  type: number
}

export interface IPackageStatus {
  id: string
  exists: boolean
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
  addresses: IContactAddress[]
  emails: IContactEmail[]
  websites: IContactWebsite[]
  events: IContactEvent[]
  ims: IContactIm[]
  tags: ITag[]
}

export interface ICallGeo {
  country: string
  numberType: string
  carrier: string
  description: string
}

export type CallType =
  | 'INCOMING' | 'OUTGOING' | 'MISSED' | 'VOICEMAIL' | 'REJECTED'
  | 'BLOCKED' | 'ANSWERED_EXTERNALLY' | 'UNKNOWN'

export interface ICall extends IData {
  id: string
  name: string
  number: string
  durationSec: number
  accountId: string
  startedAt: string
  photoId: string
  type: CallType
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
  durationMs: number
}

export interface IImage extends IMedia {
  takenAt?: string
}

export interface IVideo extends IMedia {
  durationMs: number
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

export interface IAudioItem {
  title: string
  artist: string
  path: string
  durationMs: number
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
  /** Null for direct messages (channel posts carry the channel id). */
  channelId: string | null
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

export interface IFeedEntryCount {
  id: string
  count: number
}

export interface IImageItem extends IImage {
  fileId: string
}
export interface IVideoItem extends IVideo {
  fileId: string
}
export interface IAudioWithFileId extends IAudio {
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
  postedAt: string
  silent: boolean
  title: string
  body: string
  icon: string
  actions: string[]
  replyActions: string[]
}

export interface IClipboard extends IData {
  id: string
  text: string
  source: string
  label: string
  sensitive: boolean
  createdAt: string
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
  diskId: string
  /** Partition-only metadata (NAS `mounts` also returns unmounted
   *  partitions; volumes have no `path` from the server). */
  partitionNum?: number | null
  label?: string | null
  uuid?: string | null
}

export interface IStorageDisk {
  id: string
  name: string
  path: string
  sizeBytes: number
  removable: boolean
  model: string | null
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

/**
 * Media-index scan lifecycle (plain-nas `scanProgress.state` and the
 * `media_scan_progress` WS payload share the same values).
 */
export type ScanState = 'IDLE' | 'RUNNING' | 'PAUSED' | 'STOPPED'

export interface IScanProgress {
  indexed: number
  pending: number
  total: number
  state: ScanState
}

export interface IApp {
  clientId: string
  urlToken: string
  httpPort: number
  httpsPort: number
  appDir: string
  deviceName: string
  deviceType: DeviceType
  features: string[]
  channel: AppChannelType
  permissions: string[]
  downloadsDir: string
  developerMode: boolean
  debug: boolean
}

export interface IBreadcrumbItem {
  path: string
  name: string
}

export interface IUploadMergeResultEvent {
  fileId: string
  ok: boolean
  value?: string
  mergedSize?: number
  error?: string
}
