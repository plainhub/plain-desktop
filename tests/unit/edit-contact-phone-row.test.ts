import { readFileSync } from 'node:fs'
import { compileStyle, parse } from '@vue/compiler-sfc'
import { expect, it, vi } from 'vitest'
import { defineComponent, h, reactive } from 'vue'
import EditContactModal from '@/components/EditContactModal.vue'
import { click, clientTemplate, flush, ModalShell, nodes, renderView } from '../helpers/vue-node-renderer'

vi.mock('@/components/modal', () => ({ popModal: vi.fn() }))
vi.mock('@/hooks/edit-contact', () => ({
  useEditContact: (data: { phoneNumbers: { type: string; value: string }[] }) => ({
    editItem: reactive({ ...data, emails: [], addresses: [], websites: [], ims: [] }),
    types: { phoneNumberTypes: [] },
    createTypeOptions: () => [],
    deleteField: (items: unknown[], index: number) => items.splice(index, 1),
  }),
}))

clientTemplate(EditContactModal, 'src/components/EditContactModal.vue')

it('keeps phone type and number adjacent, releasing the delete slot when only one number remains', async () => {
  const view = renderView(EditContactModal, {
    data: { phoneNumbers: [{ type: 'MOBILE', value: '+86 123 4567 8900' }, { type: 'HOME', value: '12345' }] },
  }, {
    VModal: ModalShell,
    VSelect: () => h('select'),
    VTextField: defineComponent({ setup: () => () => h('input') }),
    VDropdown: () => null,
    VFilledButton: () => h('button'),
    VOutlinedButton: () => h('button'),
  })
  const phoneRows = () => nodes(view.root).filter(node => node.tag === 'div' && node.children.some(child => child.props.placeholder === 'telephone'))
  const controls = () => phoneRows().map(row => row.children.filter(child => ['select', 'input', 'button', 'div'].includes(child.tag)))
  try {
    expect(controls().map(row => row.map(child => child.tag))).toEqual([['select', 'input', 'button'], ['select', 'input', 'button']])
    expect(phoneRows().map(row => row.props.class)).toEqual(['form-row phone-row', 'form-row phone-row'])
    expect(controls()[0]?.[0]?.props.class).toBe('phone-type')
    expect(controls()[0]?.[1]?.props.class).toBeUndefined()
    click(controls()[1]![2]!)
    await flush()
    expect(controls().map(row => row.map(child => child.tag))).toEqual([['select', 'input']])
    expect(controls()[0]?.[1]?.props.modelValue).toBe('+86 123 4567 8900')
  } finally {
    view.unmount()
  }
})

it('keeps the phone type compact and allows the number field to use the remaining row width', () => {
  const filename = 'src/components/EditContactModal.vue'
  const { descriptor } = parse(readFileSync(new URL(`../../${filename}`, import.meta.url), 'utf8'))
  const style = descriptor.styles[0]!
  const { code, errors } = compileStyle({
    filename, id: 'data-v-test', source: style.content, scoped: style.scoped, preprocessLang: 'scss',
  })
  expect(errors).toEqual([])
  expect(code).toMatch(/\.phone-type\[data-v-test\]\s*\{[^}]*flex:\s*none;[^}]*max-inline-size:/)
  expect(code).toMatch(/\.phone-row\[data-v-test\]\s*>\s*\.text-field-wrapper\s*\{[^}]*min-inline-size:\s*0;/)
})
