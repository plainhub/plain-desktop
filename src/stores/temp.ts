import type { ISource } from '@/components/lightbox/types'
import type { IFile } from '@/lib/file'
import type { IDocExtGroup } from '@/lib/interfaces'
import { defineCrossWindowStore } from '@/lib/cross-window-store'
import type { IApp } from '@/lib/interfaces'

export interface IUploadItem {
  id: string
  batchId?: string
  createdAt?: string
  dir: string
  baseDir?: string
  relativePath?: string
  fileName: string
  file: File
  uploadedSize: number
  status: string
  error: string
  pausing?: boolean
  xhr?: XMLHttpRequest
  xhrs?: Set<XMLHttpRequest>
  fileId?: string
  fileHash?: string
  isAppFile?: boolean
  isChunked?: boolean
  uploadedChunks?: number[]
  uploadSpeed?: number // bytes per second
  lastUploadedSize?: number
  lastUpdateTime?: number
}

export interface ILightBox {
  sources: ISource[]
  visible: boolean
  index: number
  readOnly: boolean
}

export interface IDataCounter {
  messages: number
  contacts: number
  calls: number
  videos: number
  videosTrash: number
  images: number
  imagesTrash: number
  audios: number
  audiosTrash: number
  packages: number
  packagesSystem: number
  notes: number
  notesTrash: number
  docs: number
  docsTrash: number
  docExtGroups: IDocExtGroup[]
  feedEntries: number
  feedEntriesToday: number
  total: number
  free: number
}

// data will be lost when page refreshed, or the data use different way to store data
export type TempState = {
  app: IApp // store the appFragment result
  urlTokenKey: Uint8Array | null
  uploads: IUploadItem[]
  selectedFiles: IFile[]
  audioPlaying: boolean
  lightbox: ILightBox
  counter: IDataCounter
  feedsSyncing: boolean
}

export const useTempStore = defineCrossWindowStore<'temp', TempState>('temp', {
  state: () =>
    ({
      app: {
        clientId: '',
      } as IApp,
      urlTokenKey: null,
      uploads: [],
      selectedFiles: [],
      audioPlaying: false,
      lightbox: { sources: [], visible: false, index: -1, readOnly: false },
      counter: {
        messages: -1,
        contacts: -1,
        calls: -1,
        videos: -1,
        videosTrash: -1,
        images: -1,
        imagesTrash: -1,
        audios: -1,
        audiosTrash: -1,
        packages: -1,
        packagesSystem: -1,
        notes: -1,
        notesTrash: -1,
        docs: -1,
        docsTrash: -1,
        docExtGroups: [],
        feedEntries: -1,
        feedEntriesToday: -1,
        total: -1,
        free: -1,
      },
      feedsSyncing: false,
    }) as TempState,
}, {
  // Only these keys are mirrored across windows/tabs via BroadcastChannel.
  // Everything else (app config, uploads, lightbox, urlTokenKey) stays
  // strictly per-window because it's either per-device state or local UI.
  syncKeys: ['counter', 'audioPlaying', 'feedsSyncing'],
})