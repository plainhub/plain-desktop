import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import VCopyButton from '@/components/base/VCopyButton.vue'
import { copyTextToClipboard } from '@/lib/clipboard'

vi.mock('@/lib/clipboard', () => ({ copyTextToClipboard: vi.fn() }))

const copyMock = vi.mocked(copyTextToClipboard)

async function settle() {
  await Promise.resolve()
  await Promise.resolve()
  await nextTick()
}

function mountButton(text: string) {
  return mount(VCopyButton, {
    props: { text },
    global: {
      config: { globalProperties: { $t: (key: string) => key } },
    },
  })
}

describe('VCopyButton', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  })

  afterEach(() => {
    vi.useRealTimers()
    copyMock.mockReset()
  })

  it('renders a plain copy button before copying', () => {
    const wrapper = mountButton('hello')
    const button = wrapper.find('button.v-copy-button')
    expect(button.exists()).toBe(true)
    expect(button.classes()).not.toContain('copied')
    expect(button.attributes('aria-label')).toBe('copy')
  })

  it('marks copied after a successful copy and reverts after 1.5s', async () => {
    copyMock.mockResolvedValue(true)
    const wrapper = mountButton('hello')
    await wrapper.find('button').trigger('click')
    await settle()

    expect(copyMock).toHaveBeenCalledWith('hello')
    expect(wrapper.find('button.copied').exists()).toBe(true)
    expect(wrapper.find('.copied-ico').exists()).toBe(true)

    vi.advanceTimersByTime(1499)
    await nextTick()
    expect(wrapper.find('button.copied').exists()).toBe(true)

    vi.advanceTimersByTime(1)
    await nextTick()
    expect(wrapper.find('button.copied').exists()).toBe(false)
    expect(wrapper.find('.copied-ico').exists()).toBe(false)
  })

  it('stays plain when the copy fails', async () => {
    copyMock.mockResolvedValue(false)
    const wrapper = mountButton('hello')
    await wrapper.find('button').trigger('click')
    await settle()

    expect(wrapper.find('button.copied').exists()).toBe(false)
    expect(wrapper.find('.copied-ico').exists()).toBe(false)
  })

  it('restarts the revert timer on re-copy', async () => {
    copyMock.mockResolvedValue(true)
    const wrapper = mountButton('hello')
    await wrapper.find('button').trigger('click')
    await settle()

    vi.advanceTimersByTime(1000)
    await wrapper.find('button').trigger('click')
    await settle()

    vi.advanceTimersByTime(1000)
    await nextTick()
    expect(wrapper.find('button.copied').exists()).toBe(true)

    vi.advanceTimersByTime(500)
    await nextTick()
    expect(wrapper.find('button.copied').exists()).toBe(false)
  })
})
