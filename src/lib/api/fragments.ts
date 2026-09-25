export const tagFragment = `
  fragment TagFragment on Tag {
    id
    name
    count
  }
`

export const tagSubFragment = `
  fragment TagSubFragment on Tag {
    id
    name
  }
`

export const audioItemFragment = `
  fragment AudioItemFragment on AudioItem {
    title
    artist
    path
    durationMs
  }
`

export const appFragment = `
  fragment AppFragment on App {
    clientId
    urlToken
    httpPort
    httpsPort
    appDir
    deviceName
    deviceType
    capabilities
    buildChannel
    permissions
    downloadsDir
    developerMode
    debug
  }
`

export const chatItemFragment = `
  fragment ChatItemFragment on ChatItem {
    id
    fromId
    toId
    channelId
    createdAt
    content
    status
    statusData
    data {
      ... on ChatImages {
        ids
      }
      ... on ChatFiles {
        ids
      }
      ... on ChatText {
        linkPreviewImageIds
      }
    }
  }
`

export const smsFragment = `
  fragment SmsFragment on Sms {
    id
    body
    address
    serviceCenter
    sentAt
    type
    threadId
    subscriptionId
    isMms
    attachments {
      path
      contentType
      name
    }
    tags {
      ...TagSubFragment
    }
  }
  ${tagSubFragment}
`

export const smsConversationFragment = `
  fragment SmsConversationFragment on SmsConversation {
    id
    address
    snippet
    lastMessageAt
    messageCount
    read
  }
`

// This stays separate from the legacy fragment: GraphQL rejects unknown
// fields on older PlainApp servers, so callers can retry without `addresses`.
export const smsConversationWithAddressesFragment = `
  fragment SmsConversationWithAddressesFragment on SmsConversation {
    id
    address
    addresses
    snippet
    lastMessageAt
    messageCount
    read
  }
`

export const contactFragment = `
  fragment ContactFragment on Contact {
    id
    suffix
    prefix
    firstName
    middleName
    lastName
    updatedAt
    notes
    source
    thumbnailId
    starred
    phoneNumbers {
      value
      type
      label
      normalizedNumber
    }
    addresses {
      value
      type
      label
    }
    emails {
      value
      type
      label
    }
    websites {
      value
      type
      label
    }
    events {
      value
      type
      label
    }
    ims {
      value
      protocol
      customProtocol
    }
    tags {
      ...TagSubFragment
    }
  }
  ${tagSubFragment}
`

export const callFragment = `
  fragment CallFragment on Call {
    id
    name
    number
    durationSec
    accountId
    startedAt
    photoId
    type
    geo {
      country
      numberType
      carrier
      description
    }
    tags {
      ...TagSubFragment
    }
  }
  ${tagSubFragment}
`

export const fileFragment = `
  fragment FileFragment on File {
    path
    isDir
    createdAt
    updatedAt
    size
    childCount
    mediaId
  }
`

export const imageFragment = `
  fragment ImageFragment on Image {
    id
    title
    path
    size
    bucketId
    takenAt
    createdAt
    updatedAt
    tags {
      ...TagSubFragment
    }
  }
  ${tagSubFragment}
`

export const videoFragment = `
  fragment VideoFragment on Video {
    id
    title
    path
    durationMs
    size
    bucketId
    createdAt
    updatedAt
    takenAt
    tags {
      ...TagSubFragment
    }
  }
  ${tagSubFragment}
`

export const audioFragment = `
  fragment AudioFragment on Audio {
    id
    title
    artist
    path
    durationMs
    size
    bucketId
    albumFileId
    createdAt
    updatedAt
    tags {
      ...TagSubFragment
    }
  }
  ${tagSubFragment}
`

export const noteFragment = `
  fragment NoteFragment on Note {
    id
    title
    content
    deletedAt
    createdAt
    updatedAt
    tags {
      ...TagSubFragment
    }
  }
  ${tagSubFragment}
`

export const docFragment = `
  fragment DocFragment on Doc {
    id
    title
    path
    extension
    size
    bucketId
    createdAt
    updatedAt
    tags {
      ...TagSubFragment
    }
  }
  ${tagSubFragment}
`

export const feedFragment = `
  fragment FeedFragment on Feed {
    id
    name
    url
    fetchContent
    createdAt
    updatedAt
  }
`

export const feedEntryFragment = `
  fragment FeedEntryFragment on FeedEntry {
    id
    title
    url
    image
    author
    description
    content
    feedId
    rawId
    publishedAt
    createdAt
    updatedAt
    tags {
      ...TagSubFragment
    }
  }
  ${tagSubFragment}
`

export const packageFragment = `
  fragment PackageFragment on Package {
    id
    name
    type
    version
    path
    size
    certs {
      issuer
      subject
      serialNumber
      validFrom
      validTo
    }
    installedAt
    updatedAt
  }
`

export const notificationFragment = `
  fragment NotificationFragment on Notification {
    id
    onlyOnce
    isClearable
    appId
    appName
    postedAt
    silent
    title
    body
    actions
    replyActions
  }
`

export const clipboardItemFragment = `
  fragment ClipboardItemFragment on ClipboardItem {
    id
    text
    source
    label
    sensitive
    createdAt
  }
`

export const deviceInfoFragment = `
  fragment DeviceInfoFragment on DeviceInfo {
    name
    platform
    manufacturer
    model
    osName
    osVersion
    kernelVersion
    appVersion
    appBuildNumber
    language
    cpuArch
    cpuModel
    totalMemory
    totalStorage
    display {
      width
      height
      density
    }
    android {
      sdkVersion
      versionCodeName
      securityPatch
      bootloader
      fingerprint
      hardware
      radioVersion
      board
      buildBrand
      buildNumber
      device
      javaVmVersion
      glEsVersion
      buildTime
    }
  }
`

export const deviceStatusFragment = `
  fragment DeviceStatusFragment on DeviceStatus {
    uptimeSec
    batteryLevel
    charging
    temperatures {
      label
      celsius
    }
    cpuUsage
    memoryAvailable
    storageAvailable
  }
`

export const bookmarkFragment = `
  fragment BookmarkFragment on Bookmark {
    id
    url
    title
    faviconPath
    groupId
    pinned
    clickCount
    lastClickedAt
    sortOrder
    createdAt
    updatedAt
  }
`

export const bookmarkGroupFragment = `
  fragment BookmarkGroupFragment on BookmarkGroup {
    id
    name
    collapsed
    sortOrder
    itemCount
    createdAt
    updatedAt
  }
`

export const chatChannelMemberFragment = `
  fragment ChatChannelMemberFragment on ChatChannelMember {
    peerId
    status
  }
`

export const chatChannelFragment = `
  fragment ChatChannelFragment on ChatChannel {
    id
    name
    ownerId
    members {
      ...ChatChannelMemberFragment
    }
    version
    status
    createdAt
    updatedAt
  }
  ${chatChannelMemberFragment}
`
