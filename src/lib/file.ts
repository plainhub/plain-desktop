import { getFileId, getFileName, getFileExtension, getFileUrl } from './api/file'
import type { IData } from './interfaces'

const photoExtensions = ['.jpg', '.png', '.jpeg', '.bmp', '.webp', '.heic', '.heif', '.apng', '.avif', '.gif', '.tiff', '.tif', '.svg']
const videoExtensions = ['.mp4', '.mkv', '.webm', '.avi', '.3gp', '.mov', '.m4v', '.3gpp']
const audioExtensions = ['.mp3', '.wav', '.wma', '.ogg', '.m4a', '.opus', '.flac', '.aac']
const rawExtensions = ['.dng', '.orf', '.nef', '.arw', '.rw2', '.cr2', '.cr3']

export interface IFile extends IData {
  fileId: string
  path: string
  name: string
  isDir: boolean
  updatedAt: string
  createdAt: string
  extension: string
  size: number
}

export function isTextFile(name: string) {
  const v = name.toLowerCase()
  return ['.txt', '.md', '.markdown', '.json', '.xml', '.css', '.js', '.ts', '.html', '.yml', '.yaml', '.ini', '.conf', '.log'].some((it) => v.endsWith(it))
}

export function isAppFile(path: string) {
  return path.startsWith('fid:')
}

export function isImage(name: string) {
  const v = name.toLowerCase()
  return photoExtensions.some((it) => v.endsWith(it))
}

export function isSvg(name: string) {
  const v = name.toLowerCase()
  return v.endsWith('.svg')
}

export function isHeic(name: string) {
  const v = name.toLowerCase()
  return v.endsWith('.heic') || v.endsWith('.heif')
}

export function isVideo(name: string) {
  const v = name.toLowerCase()
  return videoExtensions.some((it) => v.endsWith(it))
}

export function isAudio(name: string) {
  const v = name.toLowerCase()
  return audioExtensions.some((it) => v.endsWith(it))
}

const docExtensions = ['.doc', '.docx', '.xls', '.xlsx', '.ppt', '.pptx', '.pdf', '.txt', '.md', '.csv', '.json', '.xml', '.js', '.ts', '.py', '.java', '.kt', '.swift', '.c', '.cpp', '.h', '.cs', '.go', '.rs', '.rb', '.sh', '.yaml', '.yml', '.toml', '.ini', '.cfg', '.log']

export function isDoc(name: string) {
  const v = name.toLowerCase()
  return docExtensions.some((it) => v.endsWith(it))
}

export function isRaw(name: string) {
  const v = name.toLowerCase()
  return rawExtensions.some((it) => v.endsWith(it))
}

export function canView(name: string) {
  return isImage(name) || isVideo(name) || isAudio(name)
}

export function canOpenInBrowser(name: string) {
  return ['.txt', '.pdf', '.md'].some((it) => name.endsWith(it))
}

export function getSortItems() {
  return [
    { label: 'sort_by.date_asc', value: 'DATE_ASC' },
    { label: 'sort_by.date_desc', value: 'DATE_DESC' },
    { label: 'sort_by.size_asc', value: 'SIZE_ASC' },
    { label: 'sort_by.size_desc', value: 'SIZE_DESC' },
    { label: 'sort_by.name_asc', value: 'NAME_ASC' },
    { label: 'sort_by.name_desc', value: 'NAME_DESC' },
  ]
}

export function getImageSortItems() {
  return getSortItems()
}

export function getImageGroupByItems() {
  return [
    { label: 'group_by.none', value: '' },
    { label: 'group_by.taken_time', value: 'TAKEN_AT' },
  ]
}

export function getVideoGroupByItems() {
  return [
    { label: 'group_by.none', value: '' },
    { label: 'group_by.taken_time', value: 'TAKEN_AT' },
  ]
}

export function getGroupDateKey(takenAt: string | undefined | null, createdAt: string): string {
  const d = takenAt ? new Date(takenAt) : new Date(createdAt)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

export function formatGroupDateLabel(dateKey: string): string {
  // Parse as local noon to avoid DST/timezone boundary issues
  return new Intl.DateTimeFormat(undefined, {
    weekday: 'short',
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  }).format(new Date(dateKey + 'T12:00:00'))
}

export async function getVideoData(videoFile: File): Promise<{ src: string; duration: number; thumbnail: string; width: number; height: number }> {
  return new Promise((resolve) => {
    const video = document.createElement('video')
    const canvas = document.createElement('canvas')

    const src = URL.createObjectURL(videoFile)
    video.src = src
    video.play()

    video.onloadeddata = async () => {
      const squareSize = Math.min(video.videoWidth, video.videoHeight)
      const cropX = (video.videoWidth - squareSize) / 2
      const cropY = (video.videoHeight - squareSize) / 2

      canvas.width = 200 // Set the desired width
      canvas.height = 200 // Set the desired height
      const canvasContext = canvas.getContext('2d')
      if (canvasContext) {
        canvasContext.drawImage(video, cropX, cropY, squareSize, squareSize, 0, 0, canvas.width, canvas.height)
      }

      const thumbnail = canvas.toDataURL()

      // Pause the video and clean up
      video.pause()
      video.currentTime = 0

      URL.revokeObjectURL(src)
      video.remove()
      canvas.remove()

      resolve({
        src,
        duration: Math.round(video.duration),
        thumbnail,
        width: video.videoWidth,
        height: video.videoHeight,
      })
    }
    video.onerror = () => {
      resolve({ src, duration: 0, thumbnail: '', width: 0, height: 0 })
    }
  })
}

export async function getImageData(file: File): Promise<{ src: string; width: number; height: number }> {
  return new Promise((resolve) => {
    const img = new Image()
    const src = URL.createObjectURL(file)
    img.onload = function () {
      URL.revokeObjectURL(src)
      resolve({
        src,
        width: img.width,
        height: img.height,
      })
    }
    img.src = src
    img.onerror = () => {
      resolve({ src, width: 0, height: 0 })
    }
  })
}

export function getDirFromPath(path: string) {
  const index = path.lastIndexOf('/')
  return index === -1 ? '' : path.substring(0, index)
}

export function enrichFile(item: IFile) {
  const name = getFileName(item.path.replace(/\/+$/, ''))
  const extension = item.isDir ? '' : getFileExtension(name)
  return {
    ...item,
    id: item.path,
    name,
    fileId: '',
    extension,
  }
}

/**
 * Compute a file's thumbnail URL on demand. enrichFile deliberately leaves
 * fileId empty so listing a large directory does not run thousands of ChaCha
 * encryptions up front — only the rows actually rendered pay the cost
 * (getFileId caches per path in window.fileIdMap).
 */
export function fileThumbUrl(urlTokenKey: Uint8Array | null, item: Pick<IFile, 'path' | 'name' | 'isDir'>): string {
  if (item.isDir || !(isImage(item.name) || isVideo(item.name))) return ''
  return getFileUrl(getFileId(urlTokenKey, item.path), '&w=50&h=50')
}

export const ZIP_SEPARATOR = '!zip!/'

export function isZipFile(name: string) {
  return name.toLowerCase().endsWith('.zip')
}

export function isZipPath(path: string) {
  return path.includes(ZIP_SEPARATOR)
}

export function getZipFilePath(path: string) {
  return path.split(ZIP_SEPARATOR)[0]
}

export function getZipInternalPath(path: string) {
  return path.split(ZIP_SEPARATOR)[1] ?? ''
}

export function joinZipPath(zipFilePath: string, internalPath: string) {
  return `${zipFilePath}${ZIP_SEPARATOR}${internalPath}`
}

/**
 * Determine storage type based on root path
 * @param rootPath - The root path to analyze
 * @param appPaths - Object containing various app storage paths
 * @returns Storage type string
 */
export function getStorageTypeByRootPath(
  rootPath: string,
  appPaths: {
    internalStoragePath: string
    appDir: string
    sdcardPath?: string
    usbDiskPaths: string[]
  }
): string {
  if (rootPath === appPaths.appDir) {
    return 'APP'
  } else if (rootPath === appPaths.sdcardPath) {
    return 'SDCARD'
  } else if (appPaths.usbDiskPaths.includes(rootPath)) {
    return 'USB_STORAGE'
  }
  return 'INTERNAL_STORAGE'
}
