import { describe, it, expect } from 'vitest'
import { ALL_FEATURES, DEBUG_EXCLUDED_FEATURE_IDS, GOOGLE_EXCLUDED_FEATURE_IDS, NAS_FEATURE_IDS, getAvailableFeatures } from '@/views/app-rail/features'
import { AppChannelType, DeviceType } from '@/lib/status'

describe('getAvailableFeatures', () => {
  it('carries no per-feature gating flags', () => {
    for (const feature of ALL_FEATURES) {
      expect(feature).not.toHaveProperty('requireNonGoogle')
      expect(feature).not.toHaveProperty('requireDebug')
    }
    expect(GOOGLE_EXCLUDED_FEATURE_IDS).toEqual(new Set(['apps', 'messages', 'calls']))
    expect(DEBUG_EXCLUDED_FEATURE_IDS).toEqual(new Set(['image_editor']))
  })

  it('defaults to the full non-NAS set when called without arguments', () => {
    expect(getAvailableFeatures().map((f) => f.id)).toEqual([
      'files', 'audios', 'images', 'videos', 'chat', 'docs', 'apps',
      'notes', 'feeds', 'messages', 'calls', 'contacts', 'screen_mirror',
    ])
    expect(getAvailableFeatures(undefined, AppChannelType.GOOGLE).map((f) => f.id)).not.toContain('apps')
  })

  it('returns the full set for a non-NAS device', () => {
    const ids = getAvailableFeatures(DeviceType.PHONE, AppChannelType.GITHUB).map((f) => f.id)
    expect(ids).toEqual([
      'files', 'audios', 'images', 'videos', 'chat', 'docs', 'apps',
      'notes', 'feeds', 'messages', 'calls', 'contacts', 'screen_mirror',
    ])
  })

  it('returns only the NAS whitelist for a NAS device, in definition order', () => {
    const ids = getAvailableFeatures(DeviceType.NAS, AppChannelType.GITHUB).map((f) => f.id)
    expect(ids).toEqual(['files', 'audios', 'images', 'videos', 'chat', 'docs'])
    expect(ids.every((id) => NAS_FEATURE_IDS.has(id))).toBe(true)
  })

  it('hides apps, messages and calls on the Google channel', () => {
    const github = getAvailableFeatures(DeviceType.PHONE, AppChannelType.GITHUB).map((f) => f.id)
    const google = getAvailableFeatures(DeviceType.PHONE, AppChannelType.GOOGLE).map((f) => f.id)
    expect(github).toContain('apps')
    expect(github).toContain('messages')
    expect(github).toContain('calls')
    expect(google).not.toContain('apps')
    expect(google).not.toContain('messages')
    expect(google).not.toContain('calls')
    expect(google).toContain('chat')
  })

  it('hides debug-only features unless debug is enabled (non-NAS)', () => {
    expect(getAvailableFeatures(DeviceType.PHONE, AppChannelType.GITHUB, false).some((f) => f.id === 'image_editor')).toBe(false)
    expect(getAvailableFeatures(DeviceType.PHONE, AppChannelType.GITHUB, true).some((f) => f.id === 'image_editor')).toBe(true)
  })

  it('hides debug-only features on NAS even with debug enabled', () => {
    const ids = getAvailableFeatures(DeviceType.NAS, AppChannelType.GITHUB, true).map((f) => f.id)
    expect(ids).not.toContain('image_editor')
  })
})
