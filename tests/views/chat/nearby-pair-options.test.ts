import { mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import { describe, expect, it } from 'vitest'
import NearbyPairOptions from '@/views/chat/NearbyPairOptions.vue'

const Button = defineComponent({
  setup(_, { emit, slots }) {
    return () => h('button', { onClick: () => emit('click') }, slots.default?.())
  },
})

function mountOptions() {
  return mount(NearbyPairOptions, {
    global: {
      stubs: { VFilledButton: Button, VOutlinedButton: Button },
      mocks: { $t: (key: string) => key },
    },
  })
}

describe('NearbyPairOptions', () => {
  it('starts full control login when its default choice is confirmed', async () => {
    const wrapper = mountOptions()
    await wrapper.findAll('button')[1]?.trigger('click')
    expect(wrapper.emitted('confirm')?.[0]).toEqual(['control'])
  })

  it('starts chat pairing after the user chooses chat only', async () => {
    const wrapper = mountOptions()
    await wrapper.find('input[value="chat"]').setValue()
    await wrapper.findAll('button')[1]?.trigger('click')
    expect(wrapper.emitted('confirm')?.[0]).toEqual(['chat'])
  })
})
