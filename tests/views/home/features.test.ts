import { describe, it, expect } from 'vitest'
import { getAvailableHomeFeatures } from '@/views/home/features'
import { AppChannelType, DeviceType } from '@/lib/status'

describe('getAvailableHomeFeatures', () => {
  it('defaults to the full non-NAS home set when called without arguments', () => {
    const ids = getAvailableHomeFeatures().map((f) => f.id)
    expect(ids).toContain('apps')
    expect(ids).toContain('call_phone')
    expect(ids).not.toContain('image_editor')
  })

  it('returns exactly the five NAS cards in order for a NAS device', () => {
    const features = getAvailableHomeFeatures(DeviceType.NAS, AppChannelType.GITHUB)
    expect(features.map((f) => f.id)).toEqual(['audios', 'images', 'videos', 'docs', 'files'])
    expect(features.every((f) => f.sectionType === 'feature')).toBe(true)
  })

  it('omits the call phone panel on NAS', () => {
    const ids = getAvailableHomeFeatures(DeviceType.NAS, AppChannelType.GITHUB).map((f) => f.id)
    expect(ids).not.toContain('call_phone')
  })

  it('maps count keys for the NAS cards and leaves files without one', () => {
    const byId = new Map(getAvailableHomeFeatures(DeviceType.NAS, AppChannelType.GITHUB).map((f) => [f.id, f]))
    expect(byId.get('audios')?.countKey).toBe('audios')
    expect(byId.get('images')?.countKey).toBe('images')
    expect(byId.get('videos')?.countKey).toBe('videos')
    expect(byId.get('docs')?.countKey).toBe('docs')
    expect(byId.get('files')?.countKey).toBeUndefined()
  })

  it('returns the full home set with phone panels for a non-NAS device', () => {
    const ids = getAvailableHomeFeatures(DeviceType.PHONE, AppChannelType.GITHUB).map((f) => f.id)
    expect(ids).toContain('apps')
    expect(ids).toContain('notes')
    expect(ids).toContain('feeds')
    expect(ids).not.toContain('clipboard')
    expect(ids).toContain('call_phone')
    expect(ids[0]).toBe('audios')
  })

  it('hides apps, messages and calls on the Google channel (non-NAS)', () => {
    const github = getAvailableHomeFeatures(DeviceType.PHONE, AppChannelType.GITHUB).map((f) => f.id)
    const google = getAvailableHomeFeatures(DeviceType.PHONE, AppChannelType.GOOGLE).map((f) => f.id)
    expect(github).toContain('apps')
    expect(github).toContain('messages')
    expect(github).toContain('calls')
    expect(google).not.toContain('apps')
    expect(google).not.toContain('messages')
    expect(google).not.toContain('calls')
    expect(google).toContain('call_phone')
  })

  it('hides debug-only features unless debug is enabled (non-NAS)', () => {
    const withoutDebug = getAvailableHomeFeatures(DeviceType.PHONE, AppChannelType.GITHUB, false).map((f) => f.id)
    const withDebug = getAvailableHomeFeatures(DeviceType.PHONE, AppChannelType.GITHUB, true).map((f) => f.id)
    expect(withoutDebug).not.toContain('image_editor')
    expect(withDebug).toContain('image_editor')
  })
})
