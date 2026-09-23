import { describe, it, expect } from 'vitest'
import { ALL_FEATURES, DEBUG_EXCLUDED_FEATURE_IDS, GOOGLE_EXCLUDED_FEATURE_IDS, getAvailableFeatures } from '@/views/app-rail/features'
import { AppChannelType } from '@/lib/status'
import { Capability } from '@/lib/data'

// A phone declares every capability; a NAS declares only media trash/scan.
const PHONE_FEATURES = Object.values(Capability)
const NAS_FEATURES = ['MEDIA_TRASH', 'DOC_PREVIEW', 'MEDIA_SCAN']

describe('getAvailableFeatures', () => {
  it('carries no per-feature gating flags', () => {
    for (const feature of ALL_FEATURES) {
      expect(feature).not.toHaveProperty('requireNonGoogle')
      expect(feature).not.toHaveProperty('requireDebug')
    }
    expect(GOOGLE_EXCLUDED_FEATURE_IDS).toEqual(new Set(['apps', 'messages', 'calls']))
    expect(DEBUG_EXCLUDED_FEATURE_IDS).toEqual(new Set(['image_editor']))
  })

  it('shows only always-on features until the app query resolves the feature list', () => {
    expect(getAvailableFeatures().map((f) => f.id)).toEqual(['files', 'audios', 'images', 'videos', 'chat', 'docs'])
  })

  it('returns the full set for a phone (all capabilities declared)', () => {
    const ids = getAvailableFeatures(PHONE_FEATURES, AppChannelType.GITHUB).map((f) => f.id)
    expect(ids).toEqual([
      'files', 'audios', 'images', 'videos', 'chat', 'docs', 'apps',
      'notes', 'feeds', 'messages', 'calls', 'contacts', 'screen_mirror',
    ])
  })

  it('shows media/files/chat/docs for a NAS declaration, in definition order', () => {
    const ids = getAvailableFeatures(NAS_FEATURES, AppChannelType.GITHUB).map((f) => f.id)
    expect(ids).toEqual(['files', 'audios', 'images', 'videos', 'chat', 'docs'])
  })

  it('hides apps, messages and calls on the Google channel', () => {
    const github = getAvailableFeatures(PHONE_FEATURES, AppChannelType.GITHUB).map((f) => f.id)
    const google = getAvailableFeatures(PHONE_FEATURES, AppChannelType.GOOGLE).map((f) => f.id)
    expect(github).toContain('apps')
    expect(google).not.toContain('apps')
    expect(google).not.toContain('messages')
    expect(google).not.toContain('calls')
    expect(google).toContain('chat')
  })

  it('hides debug-only features unless debug is enabled', () => {
    expect(getAvailableFeatures(PHONE_FEATURES, AppChannelType.GITHUB, false).some((f) => f.id === 'image_editor')).toBe(false)
    expect(getAvailableFeatures(PHONE_FEATURES, AppChannelType.GITHUB, true).some((f) => f.id === 'image_editor')).toBe(true)
  })
})
