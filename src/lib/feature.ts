import { Capability } from '@/lib/data'

// `capabilities` is undefined until the app query resolves (the temp store boots
// with a partial app object), so treat that as "nothing declared".
export const hasFeature = (feature: Capability, features: string[] | undefined) =>
  !!features?.includes(feature)

export const hasMediaMove = (app: { capabilities?: string[] } | undefined) =>
  hasFeature(Capability.MEDIA_MOVE, app?.capabilities)
