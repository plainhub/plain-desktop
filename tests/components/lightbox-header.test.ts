import { createApp, h } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'
import LightboxHeader from '@/components/lightbox/LightboxHeader.vue'

function mountHeader(infoVisible: boolean) {
  const events: string[] = []
  const root = document.createElement('div')
  document.body.append(root)

  const app = createApp({
    setup() {
      return () => h(LightboxHeader, {
        current: { name: 'a.jpg', path: '/a.jpg', src: '' },
        infoVisible,
        onDownload: () => events.push('download'),
        onToggleInfo: () => events.push('toggle-info'),
      })
    },
  })
  app.directive('tooltip', {})
  app.config.globalProperties.$t = (key: string) => key
  app.mount(root)

  return { app, root, events }
}

const mounted: ReturnType<typeof mountHeader>[] = []

afterEach(() => {
  for (const m of mounted.splice(0)) {
    m.app.unmount()
    m.root.remove()
  }
})

describe('LightboxHeader info toggle', () => {
  it('shows the collapse icon while the info panel is visible', async () => {
    const m = mountHeader(true)
    mounted.push(m)

    const button = m.root.querySelector('.info-btn')
    expect(button).not.toBeNull()
    expect(m.root.querySelector('.info-btn i-lucide\\:panel-right-close')).not.toBeNull()
    expect(m.root.querySelector('.info-btn i-lucide\\:panel-right-open')).toBeNull()

    ;(button as HTMLElement).dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(m.events).toEqual(['toggle-info'])
  })

  it('shows the expand icon while the info panel is hidden', () => {
    const m = mountHeader(false)
    mounted.push(m)

    expect(m.root.querySelector('.info-btn i-lucide\\:panel-right-open')).not.toBeNull()
    expect(m.root.querySelector('.info-btn i-lucide\\:panel-right-close')).toBeNull()
  })
})

describe('LightboxHeader download button', () => {
  it('offers a download action in the toolbar and emits download', async () => {
    const m = mountHeader(true)
    mounted.push(m)

    const icon = m.root.querySelector('.actions i-material-symbols\\:download-rounded')
    expect(icon).not.toBeNull()

    const button = icon?.closest('v-icon-button') as HTMLElement
    button.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(m.events).toEqual(['download'])
  })
})
