import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import VCheckCircle from '@/components/base/VCheckCircle.vue'

describe('VCheckCircle', () => {
  it('renders an empty circle when unchecked', () => {
    const wrapper = mount(VCheckCircle, { props: { checked: false } })
    expect(wrapper.find('.check-circle').exists()).toBe(true)
    expect(wrapper.find('.check-circle.checked').exists()).toBe(false)
    expect(wrapper.find('svg').exists()).toBe(false)
  })

  it('renders the check icon when checked', () => {
    const wrapper = mount(VCheckCircle, { props: { checked: true } })
    expect(wrapper.find('.check-circle.checked').exists()).toBe(true)
    expect(wrapper.find('svg path').exists()).toBe(true)
  })

  it('defaults to unchecked', () => {
    const wrapper = mount(VCheckCircle)
    expect(wrapper.find('.check-circle.checked').exists()).toBe(false)
  })
})
