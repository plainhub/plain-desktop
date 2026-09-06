export function isTauriBuildMode(mode: string, configuredAppMode?: string): boolean {
  return mode === 'tauri' || configuredAppMode === 'tauri'
}
