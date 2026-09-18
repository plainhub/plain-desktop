import { createApp, h, nextTick } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'
import BulkDownloadButton from '@/components/BulkDownloadButton.vue'
import VIconButton from '@/components/base/VIconButton.vue'
import VDropdown from '@/components/base/VDropdown.vue'

function mountButton(single: boolean) {
  const events: string[] = []
  const root = document.createElement('div')
  document.body.append(root)

  const app = createApp({
    setup() {
      return () => h(BulkDownloadButton, {
        single,
        onDownload: () => events.push('download'),
        onDownloadEach: () => events.push('download-each'),
        onDownloadZip: () => events.push('download-zip'),
      })
    },
  })
  app.component('VIconButton', VIconButton)
  app.component('VDropdown', VDropdown)
  app.directive('tooltip', {})
  app.config.globalProperties.$t = (key: string) => key
  app.mount(root)

  return { app, root, events }
}

const mounted: ReturnType<typeof mountButton>[] = []

afterEach(() => {
  for (const m of mounted.splice(0)) {
    m.app.unmount()
    m.root.remove()
  }
})

async function click(el: Element) {
  el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
  await nextTick()
}

describe('BulkDownloadButton', () => {
  it('downloads directly without a menu when single', async () => {
    const m = mountButton(true)
    mounted.push(m)

    const button = m.root.querySelector('button')
    expect(button).not.toBeNull()
    await click(button!)

    expect(m.events).toEqual(['download'])
    expect(document.querySelector('.v-dropdown-portal')).toBeNull()
  })

  it('offers individual and zip downloads in a dropdown when not single', async () => {
    const m = mountButton(false)
    mounted.push(m)

    await click(m.root.querySelector('button')!)
    const menu = document.querySelector('.v-dropdown-portal')
    expect(menu).not.toBeNull()
    const items = menu!.querySelectorAll('.dropdown-item')
    expect(items.length).toBe(2)

    await click(items[0])
    expect(m.events).toEqual(['download-each'])
    expect(document.querySelector('.v-dropdown-portal')).toBeNull()

    await click(m.root.querySelector('button')!)
    await click(document.querySelectorAll('.v-dropdown-portal .dropdown-item')[1])
    expect(m.events).toEqual(['download-each', 'download-zip'])
  })
})
