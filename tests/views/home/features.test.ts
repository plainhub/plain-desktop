import { describe, it, expect } from 'vitest'
import { getAvailableHomeFeatures } from '@/views/home/features'
import { AppChannelType } from '@/lib/status'
import { Capability } from '@/lib/data'

const PHONE_FEATURES = Object.values(Capability)
const NAS_FEATURES = ['MEDIA_TRASH', 'DOC_PREVIEW', 'MEDIA_SCAN']

describe('getAvailableHomeFeatures', () => {
  it('shows only always-on cards until the feature list resolves', () => {
    const ids = getAvailableHomeFeatures().map((f) => f.id)
    expect(ids).toContain('audios')
    expect(ids).not.toContain('apps')
    expect(ids).not.toContain('call_phone')
    expect(ids).not.toContain('image_editor')
  })

  it('returns media/docs/files cards for a NAS declaration', () => {
    const ids = getAvailableHomeFeatures(NAS_FEATURES, AppChannelType.GITHUB).map((f) => f.id)
    expect(ids).toContain('audios')
    expect(ids).toContain('images')
    expect(ids).toContain('videos')
    expect(ids).toContain('docs')
    expect(ids).toContain('files')
    expect(ids).not.toContain('apps')
    expect(ids).not.toContain('notes')
  })

  it('omits the call phone panel without the CALL_PHONE capability', () => {
    expect(getAvailableHomeFeatures(NAS_FEATURES, AppChannelType.GITHUB).map((f) => f.id)).not.toContain('call_phone')
    expect(getAvailableHomeFeatures(PHONE_FEATURES, AppChannelType.GITHUB).map((f) => f.id)).toContain('call_phone')
  })

  it('maps count keys for the media cards and leaves files without one', () => {
    const byId = new Map(getAvailableHomeFeatures(NAS_FEATURES, AppChannelType.GITHUB).map((f) => [f.id, f]))
    expect(byId.get('audios')?.countKey).toBe('audios')
    expect(byId.get('docs')?.countKey).toBe('docs')
    expect(byId.get('files')?.countKey).toBeUndefined()
  })

  it('returns the full home set for a phone declaration', () => {
    const ids = getAvailableHomeFeatures(PHONE_FEATURES, AppChannelType.GITHUB).map((f) => f.id)
    expect(ids).toContain('apps')
    expect(ids).toContain('notes')
    expect(ids).toContain('feeds')
    expect(ids).toContain('call_phone')
    expect(ids[0]).toBe('audios')
  })

  it('hides apps, messages and calls on the Google channel', () => {
    const google = getAvailableHomeFeatures(PHONE_FEATURES, AppChannelType.GOOGLE).map((f) => f.id)
    expect(google).not.toContain('apps')
    expect(google).not.toContain('messages')
    expect(google).not.toContain('calls')
    expect(google).toContain('call_phone')
  })

  it('hides debug-only features unless debug is enabled', () => {
    expect(getAvailableHomeFeatures(PHONE_FEATURES, AppChannelType.GITHUB, false).map((f) => f.id)).not.toContain('image_editor')
    expect(getAvailableHomeFeatures(PHONE_FEATURES, AppChannelType.GITHUB, true).map((f) => f.id)).toContain('image_editor')
  })
})
