import { isLocalMode as _isLocalMode } from './current'

export { isLocalMode } from './current'

export function isLocalModeAllowed(): boolean {
  return __IS_TAURI__ && _isLocalMode()
}
