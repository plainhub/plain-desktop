import { afterEach, describe, expect, it, vi } from 'vitest'
import { addRecentUploadDir, getRecentUploadDirs, pickUploadDir } from '@/lib/upload/pick-upload-dir'

const promptModal = vi.hoisted(() => vi.fn())
vi.mock('@/components/modal', () => ({
  promptModal,
  popModal: vi.fn(),
  Modal: { EVENT_PROMPT: 'prompt' },
}))

afterEach(() => {
  localStorage.clear()
  promptModal.mockReset()
})

describe('recent upload dirs', () => {
  it('records picked dirs most-recent-first without duplicates', async () => {
    promptModal.mockResolvedValue('/sdcard/DCIM')
    await pickUploadDir({})
    promptModal.mockResolvedValue('/sdcard/Music')
    await pickUploadDir({})
    promptModal.mockResolvedValue('/sdcard/DCIM')
    await pickUploadDir({})

    expect(getRecentUploadDirs()).toEqual(['/sdcard/DCIM', '/sdcard/Music'])
  })

  it('caps the list at eight entries', () => {
    for (let i = 0; i < 10; i++) addRecentUploadDir(`/sdcard/dir${i}`)
    const dirs = getRecentUploadDirs()
    expect(dirs.length).toBe(8)
    expect(dirs[0]).toBe('/sdcard/dir9')
  })

  it('passes recent dirs to the picker and applies the picked value', async () => {
    addRecentUploadDir('/sdcard/Pictures')
    const setValue = vi.fn()
    promptModal.mockResolvedValue('/sdcard/Camera')

    const result = await pickUploadDir({ title: 't', setValue })

    expect(result).toBe('/sdcard/Camera')
    expect(promptModal.mock.calls[0][1].recentDirs).toEqual(['/sdcard/Pictures'])
    expect(setValue).toHaveBeenCalledWith('/sdcard/Camera')
  })

  it('keeps recents untouched when the picker is dismissed', async () => {
    addRecentUploadDir('/sdcard/DCIM')
    promptModal.mockResolvedValue(undefined)

    const result = await pickUploadDir({})

    expect(result).toBeUndefined()
    expect(getRecentUploadDirs()).toEqual(['/sdcard/DCIM'])
  })
})
