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
