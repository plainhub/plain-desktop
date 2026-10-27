import { get as browserGet, set as browserSet } from '@/lib/prefs/browser'
import { readNativeDeviceIdentity } from './native-settings'
import { shortUUID } from '@/lib/strutil'

const REMOTE_KEY = '__bound_client_id__'
const WINDOW_ID_KEY = '__window_id__'

let desktopClientId = ''

export function ensureBrowserClientId(): void {
  if (!browserGet('client_id', '')) browserSet('client_id', shortUUID())
}

export async function preloadClientIdentity(): Promise<void> {
  desktopClientId = (await readNativeDeviceIdentity()).clientId
}

let cachedRemote: string | null | undefined
let cachedWindowId: string | null

function readSession(key: string): string | null {
  try {
    return sessionStorage.getItem(key)
  } catch {
    return null
  }
}

function writeSession(key: string, value: string): void {
  try {
    if (value) sessionStorage.setItem(key, value)
    else sessionStorage.removeItem(key)
  } catch {
    // sessionStorage may be disabled (private mode quirks); fall through.
  }
}

export function getDesktopClientId(): string {
  return __IS_TAURI__ ? desktopClientId : browserGet('client_id', '')
}

export function getRemoteClientId(): string {
  if (cachedRemote !== undefined) return cachedRemote ?? ''
  const v = readSession(REMOTE_KEY) ?? ''
  cachedRemote = v
  return v
}

export function setRemoteClientId(id: string): void {
  cachedRemote = id
  writeSession(REMOTE_KEY, id)
}

export function clearRemoteClientId(): void {
  cachedRemote = ''
  writeSession(REMOTE_KEY, '')
}

export function getActiveClientId(): string {
  return getRemoteClientId() || getDesktopClientId()
}

export function isLocalClientId(id: string): boolean {
  if (!id) return true
  const desktop = getDesktopClientId()
  return !desktop || id === desktop
}

export function getWindowId(): string {
  if (cachedWindowId) return cachedWindowId
  let id = readSession(WINDOW_ID_KEY)
  if (!id) {
    id = `w_${Math.random().toString(36).slice(2, 10)}_${Date.now().toString(36)}`
    writeSession(WINDOW_ID_KEY, id)
  }
  cachedWindowId = id
  return id
}

export function __resetWindowIdForTest(): void {
  cachedWindowId = null
}

export function applyUrlClientId(): void {
  try {
    const url = new URL(window.location.href)
    const cid = url.searchParams.get('__cid')
    if (!cid) return
    if (isLocalClientId(cid)) {
      url.searchParams.delete('__cid')
    } else {
      setRemoteClientId(cid)
      url.searchParams.delete('__cid')
    }
    const query = url.searchParams.toString()
    const next = url.pathname + (query ? `?${query}` : '') + url.hash
    window.history.replaceState({}, '', next)
  } catch {
    // non-DOM environment or weird URL
  }
}