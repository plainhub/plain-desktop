import { createApp, h, nextTick } from 'vue'
import { createI18n } from 'vue-i18n'
import { afterEach, describe, expect, it } from 'vitest'
import LightboxQualityDropdown from '@/components/lightbox/LightboxQualityDropdown.vue'

function mountDropdown(modelValue: 'fast' | 'original') {
  const values: string[] = []
  const root = document.createElement('div')
  document.body.append(root)

  const app = createApp({
    setup() {
      return () =>
        h(LightboxQualityDropdown, {
          modelValue,
          'onUpdate:modelValue': (value: string) => values.push(value),
        })
    },
  })
  app.use(createI18n({ legacy: false, missingWarn: false, fallbackWarn: false }))
  app.mount(root)

  return { app, root, values }
}

const mounted: ReturnType<typeof mountDropdown>[] = []

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

describe('LightboxQualityDropdown', () => {
  it('shows the current quality as the chip label', () => {
    const m = mountDropdown('fast')
    mounted.push(m)

    expect(m.root.querySelector('.v-chip-select__label')?.textContent).toBe('image_quality_fast')
    expect(document.querySelector('.v-dropdown-portal')).toBeNull()
  })

  it('emits the picked quality and closes the menu', async () => {
    const m = mountDropdown('fast')
    mounted.push(m)

    await click(m.root.querySelector('.v-chip-select')!)

    const items = document.querySelectorAll('.v-dropdown-portal .dropdown-item')
    expect(items.length).toBe(2)

    await click(items[1]!)

    expect(m.values).toEqual(['original'])
    expect(document.querySelector('.v-dropdown-portal')).toBeNull()
    expect(m.root.querySelector('.v-chip-select__label')?.textContent).toBe('image_quality_fast')
  })
})
