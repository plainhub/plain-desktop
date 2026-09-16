export enum MemberStatus {
  JOINED = 'JOINED',
  PENDING = 'PENDING',
}

export enum ChannelStatus {
  JOINED = 'JOINED',
  LEFT = 'LEFT',
  KICKED = 'KICKED',
}

export enum PeerStatus {
  PAIRED = 'PAIRED',
  UNPAIRED = 'UNPAIRED',
  CHANNEL = 'CHANNEL',
}

export enum ChatStatus {
  SENT = 'SENT',
  FAILED = 'FAILED',
  PARTIAL = 'PARTIAL',
  PENDING = 'PENDING',
}

export enum MessageType {
  TEXT = 'TEXT',
  IMAGES = 'IMAGES',
  FILES = 'FILES',
  SHARE = 'SHARE',
}

export enum DeviceType {
  NAS = 'NAS',
  PHONE = 'PHONE',
  TABLET = 'TABLET',
  COMPUTER = 'COMPUTER',
  TV = 'TV',
  OTHER = 'OTHER',
}

export enum SessionType {
  WEB = 'WEB',
  CUSTOM = 'CUSTOM',
}

export enum PackageType {
  SYSTEM = 'SYSTEM',
  USER = 'USER',
}

export enum DriveType {
  INTERNAL_STORAGE = 'INTERNAL_STORAGE',
  SDCARD = 'SDCARD',
  USB_STORAGE = 'USB_STORAGE',
  APP = 'APP',
}

export enum AppChannelType {
  GITHUB = 'GITHUB',
  GOOGLE = 'GOOGLE',
  FDROID = 'FDROID',
}

export enum ScreenMirrorMode {
  NORMAL = 'NORMAL',
  WIDE = 'WIDE',
  FIT = 'FIT',
}

export enum ImageSearchStatusType {
  UNAVAILABLE = 'UNAVAILABLE',
  DOWNLOADING = 'DOWNLOADING',
  LOADING = 'LOADING',
  READY = 'READY',
  ERROR = 'ERROR',
}

export enum ChannelSystemMessageType {
  INVITE = 'INVITE',
  INVITE_ACCEPT = 'INVITE_ACCEPT',
  INVITE_DECLINE = 'INVITE_DECLINE',
  UPDATE = 'UPDATE',
  KICK = 'KICK',
  LEAVE = 'LEAVE',
}

export enum ChannelSystemMessageAction {
  INVITE = 'INVITE',
  UPDATE = 'UPDATE',
  KICK = 'KICK',
}
