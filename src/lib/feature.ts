import { Capability } from '@/lib/data'

// `capabilities` is undefined until the app query resolves (the temp store boots
// with a partial app object), so treat that as "nothing declared".
export const hasFeature = (feature: Capability, features: string[] | undefined) =>
  !!features?.includes(feature)

/** Trash availability is the server's own declaration (Android R+ phone,
 *  NAS filesystem trash) — no client-side OS sniffing. */
export const hasMediaTrash = (app: { capabilities?: string[] } | undefined) =>
  hasFeature(Capability.MEDIA_TRASH, app?.capabilities)
