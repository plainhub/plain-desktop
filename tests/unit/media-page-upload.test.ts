import { beforeEach, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { DataType } from '@/lib/data'
import type { IUploadItem } from '@/stores/temp'
import { useMediaPageUpload } from '@/hooks/media-page-upload'

const state = vi.hoisted(() => ({
  main: { uploadDirs: {} as Record<string, string> },
  picker: vi.fn(),
  mounts: [] as { driveType: string; path: string }[],
  temp: {} as { uploads: { value: IUploadItem[] } },
}))

vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/stores/main', () => ({ useMainStore: () => state.main }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => state.temp }))
vi.mock('@/hooks/files', () => ({ useMounts: () => ({ mounts: ref(state.mounts) }) }))
vi.mock('@/lib/upload/pick-upload-dir', () => ({ pickUploadDir: state.picker }))

function setup() {
  const page = useMediaPageUpload({
    dataType: DataType.IMAGE, filter: {}, buckets: ref([]),
    fileFilter: () => true, uploadModalId: 'fixture-upload', uploadStorageKey: 'images',
  })
  const fileClick = vi.fn()
  const folderClick = vi.fn()
  page.fileInput.value = { value: '', click: fileClick } as unknown as HTMLInputElement
  page.dirFileInput.value = { value: '', click: folderClick } as unknown as HTMLInputElement
  return { page, fileClick, folderClick }
}

function selection(relativePath = '') {
  const file = new File(['sample'], 'photo.png')
  Object.defineProperty(file, 'webkitRelativePath', { value: relativePath })
  return { target: { files: [file] } } as unknown as Event
}

beforeEach(() => {
  state.main.uploadDirs = {}
  state.mounts = []
  state.temp = { uploads: ref([]) }
  state.picker.mockReset()
})

it.each([
  { kind: 'file', relativePath: '', expectedDir: '/photos' },
  { kind: 'folder', relativePath: 'album/photo.png', expectedDir: '/photos/album' },
])('opens the $kind chooser without a default, then resolves the upload destination', async ({ kind, relativePath, expectedDir }) => {
  const { page, fileClick, folderClick } = setup()
  state.picker.mockResolvedValue('/photos')
  expect(page.uploadDir.value).toBe('')
  if (kind === 'file') {
    page.uploadFilesClick()
    expect(fileClick).toHaveBeenCalledOnce()
    expect(folderClick).not.toHaveBeenCalled()
    await page.uploadChanged(selection(relativePath))
  } else {
    page.uploadDirClick()
    expect(folderClick).toHaveBeenCalledOnce()
    expect(fileClick).not.toHaveBeenCalled()
    await page.dirUploadChanged(selection(relativePath))
  }
  expect(state.picker).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ title: 'save_location' }))
  expect(state.temp.uploads.value).toMatchObject([{ dir: expectedDir, baseDir: '/photos' }])
})

it('does not enqueue an upload when destination selection is cancelled', async () => {
  const { page } = setup()
  state.picker.mockResolvedValue(undefined)
  page.uploadFilesClick()
  await page.uploadChanged(selection())
  expect(state.picker).toHaveBeenCalledOnce()
  expect(state.temp.uploads.value).toEqual([])
})

it('uses a configured destination without asking again', async () => {
  state.main.uploadDirs.images = '/saved'
  const { page, fileClick } = setup()
  page.uploadFilesClick()
  await page.uploadChanged(selection())
  expect(fileClick).toHaveBeenCalledOnce()
  expect(state.picker).not.toHaveBeenCalled()
  expect(state.temp.uploads.value).toMatchObject([{ dir: '/saved' }])
})
