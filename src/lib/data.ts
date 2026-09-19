export enum DataType {
  AUDIO = 'AUDIO',
  VIDEO = 'VIDEO',
  IMAGE = 'IMAGE',
  PACKAGE = 'PACKAGE',
  CALL = 'CALL',
  CONTACT = 'CONTACT',
  SMS = 'SMS',
  NOTE = 'NOTE',
  BOOK = 'BOOK',
  FEED_ENTRY = 'FEED_ENTRY',
  DOC = 'DOC',
}

// String values match the server's DeviceFeature GraphQL enum items.
export enum FEATURE {
  MEDIA_TRASH = 'MEDIA_TRASH',
  MIRROR_AUDIO = 'MIRROR_AUDIO',
}
