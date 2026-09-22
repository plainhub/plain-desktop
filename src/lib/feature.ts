import { DeviceFeature } from '@/lib/data'

// `features` is undefined until the app query resolves (the temp store boots
// with a partial app object), so treat that as "nothing declared".
export const hasFeature = (feature: DeviceFeature, features: string[] | undefined) =>
  !!features?.includes(feature)

/** Trash availability is the server's own declaration (Android R+ phone,
 *  NAS filesystem trash) — no client-side OS sniffing. */
export const hasMediaTrash = (app: { features?: string[] } | undefined) =>
  hasFeature(DeviceFeature.MEDIA_TRASH, app?.features)
