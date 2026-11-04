import { initMutation, logoutGQL, runMutation } from '@/lib/api/mutation'
import { clearCurrentSession } from '@/lib/device/current'
import { clearForLogout } from '@/lib/prefs/browser'

export function useBrowserLogout() {
  const { mutate } = initMutation({ document: logoutGQL })
  return async () => {
    if (!await runMutation(mutate)) return
    await clearCurrentSession()
    clearForLogout()
    window.location.reload()
  }
}
