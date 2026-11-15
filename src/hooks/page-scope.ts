import { onActivated, onDeactivated, onUnmounted } from 'vue'

export interface PageScope {
  readonly active: boolean
  onDispose: (cleanup: () => void) => void
}

export function usePageScope(start: (scope: PageScope) => void) {
  let dispose: (() => void) | undefined
  function activate() {
    if (dispose) return
    const cleanups: (() => void)[] = []
    const scope = {
      active: true,
      onDispose(cleanup: () => void) {
        if (scope.active) cleanups.push(cleanup)
        else cleanup()
      },
    }
    dispose = () => {
      scope.active = false
      for (const cleanup of cleanups) cleanup()
      cleanups.length = 0
    }
    start(scope)
  }
  function deactivate() {
    const cleanup = dispose
    dispose = undefined
    cleanup?.()
  }
  activate()
  onActivated(activate)
  onDeactivated(deactivate)
  onUnmounted(deactivate)
  return () => {
    if (!dispose) return
    deactivate()
    activate()
  }
}
