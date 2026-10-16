import { Capability } from '@/lib/data'

// `capabilities` is undefined until the app query resolves (the temp store boots
// with a partial app object), so treat that as "nothing declared".
export const hasFeature = (feature: Capability, features: string[] | undefined) =>
  !!features?.includes(feature)

export const hasMediaTrash = (app: { capabilities?: string[] } | undefined) =>
  hasFeature(Capability.MEDIA_TRASH, app?.capabilities)

export const hasMediaDelete = (app: { capabilities?: string[] } | undefined) =>
  hasFeature(Capability.MEDIA_DELETE, app?.capabilities)

export const hasMediaMove = (app: { capabilities?: string[] } | undefined) =>
  hasFeature(Capability.MEDIA_MOVE, app?.capabilities)

export const hasFileExport = (app: { capabilities?: string[] } | undefined) =>
  hasFeature(Capability.FILE_EXPORT, app?.capabilities)

export const hasFileReveal = (app: { capabilities?: string[] } | undefined) =>
  hasFeature(Capability.FILE_REVEAL, app?.capabilities)

export const hasFileClipboard = (app: { capabilities?: string[] } | undefined) =>
  hasFeature(Capability.FILE_CLIPBOARD, app?.capabilities)
