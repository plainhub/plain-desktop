import { describe, it, expect } from 'vitest'
import { uploadErrorKey, uploadErrorText, isTransientUploadError } from '@/lib/upload/errors'

describe('uploadErrorKey', () => {
  it('maps GqlError keys and literal client strings to i18n keys', () => {
    expect(uploadErrorKey('connection_timeout')).toBe('connection_timeout')
    expect(uploadErrorKey('network_error')).toBe('network_error')
    expect(uploadErrorKey('Network error')).toBe('network_error')
    expect(uploadErrorKey('desktop_access_disabled')).toBe('desktop_access_disabled')
    expect(uploadErrorKey('unauthorized')).toBe('upload_unauthorized')
    expect(uploadErrorKey('Failed to merge chunks')).toBe('upload_merge_failed')
  })

  it('pattern-matches chunk and merge-size failures', () => {
    expect(uploadErrorKey('Failed to upload: chunk 3, chunk 7')).toBe('upload_chunk_failed')
    expect(uploadErrorKey('Server merged size 100 != expected 120')).toBe('upload_merge_size_mismatch')
  })

  it('returns undefined for server text and empty strings', () => {
    expect(uploadErrorKey('Missing chunk 2')).toBeUndefined()
    expect(uploadErrorKey('')).toBeUndefined()
  })
})

describe('uploadErrorText', () => {
  it('translates known keys and passes unknown text through', () => {
    const t = (key: string) => `T(${key})`
    expect(uploadErrorText(t, 'connection_timeout')).toBe('T(connection_timeout)')
    expect(uploadErrorText(t, 'Failed to upload: chunk 1')).toBe('T(upload_chunk_failed)')
    expect(uploadErrorText(t, 'Missing chunk 2')).toBe('Missing chunk 2')
  })
})

describe('isTransientUploadError', () => {
  it('classifies transport-level errors as retryable', () => {
    expect(isTransientUploadError('connection_timeout')).toBe(true)
    expect(isTransientUploadError('Network error')).toBe(true)
    expect(isTransientUploadError('Failed to upload: chunk 0')).toBe(true)
    expect(isTransientUploadError('Failed to merge chunks')).toBe(true)
  })

  it('classifies verdicts and unknown server text as final', () => {
    expect(isTransientUploadError('unauthorized')).toBe(false)
    expect(isTransientUploadError('desktop_access_disabled')).toBe(false)
    expect(isTransientUploadError('Server merged size 1 != expected 2')).toBe(false)
    expect(isTransientUploadError('File name is invalid')).toBe(false)
    expect(isTransientUploadError('')).toBe(false)
  })
})
