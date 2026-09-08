import { createApp, h, nextTick } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'
import ScreenMirrorHeaderActions from '@/views/screen-mirror/ScreenMirrorHeaderActions.vue'
import VCircularProgress from '@/components/base/VCircularProgress.vue'
import VDropdown from '@/components/base/VDropdown.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import { AppChannelType } from '@/lib/status'

function mountActions(extra: { audioSupported?: boolean, osVersion?: number } = {}) {
  const root = document.createElement('div')
  document.body.append(root)
  const emitted: string[] = []
  const app = createApp({
    setup() {
      return () => h(ScreenMirrorHeaderActions, {
        mirroring: true,
        idle: false,
        showLoading: false,
        stopServiceLoading: false,
        qualityMode: 'HD',
        recording: false,
        recordingTime: '00:00',
        controlEnabled: false,
        relaunchAppLoading: false,
        channel: AppChannelType.GITHUB,
        paused: false,
        isFullscreen: false,
        muted: true,
        audioSupported: extra.audioSupported ?? true,
        osVersion: extra.osVersion ?? 30,
        onToggleMute: () => emitted.push('toggleMute'),
      })
    },
  })
  app.component('VDropdown', VDropdown)
  app.component('VIconButton', VIconButton)
  app.component('VCircularProgress', VCircularProgress)
  app.config.globalProperties.$t = (key: string) => key
  app.directive('tooltip', {})
  app.mount(root)

  return { app, root, emitted }
}

const mountedApps: ReturnType<typeof mountActions>[] = []

afterEach(() => {
  for (const mounted of mountedApps.splice(0)) {
    mounted.app.unmount()
    mounted.root.remove()
  }
})

describe('ScreenMirrorHeaderActions sound button', () => {
  it('keeps a disabled sound button that opens the Android 10 hint when the phone is below Android 10', async () => {
    const mounted = mountActions({ osVersion: 9 })
    mountedApps.push(mounted)

    const sound = mounted.root.querySelector('.action-group button.btn-icon[disabled]')
    expect(sound).not.toBeNull()

    sound!.parentElement!.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
    await nextTick()

    const hint = document.body.querySelector('.v-dropdown-portal .warning-text')
    expect(hint?.textContent).toBe('mirror_audio_not_supported')
    expect(mounted.emitted).toEqual([])
  })

  it('disables the sound button when the browser cannot decode audio', () => {
    const mounted = mountActions({ audioSupported: false })
    mountedApps.push(mounted)

    expect(mounted.root.querySelector('.action-group button.btn-icon[disabled]')).not.toBeNull()
    expect(mounted.emitted).toEqual([])
  })

  it('emits toggleMute from the enabled sound button when audio is available', () => {
    const mounted = mountActions()
    mountedApps.push(mounted)

    const buttons = mounted.root.querySelectorAll('.action-group button.btn-icon')
    expect(buttons.length).toBe(3)
    expect(buttons[1]!.hasAttribute('disabled')).toBe(false)

    buttons[1]!.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
    expect(mounted.emitted).toEqual(['toggleMute'])
  })
})
