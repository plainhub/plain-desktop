import { readFileSync } from 'node:fs'
import { compileStyle, parse } from '@vue/compiler-sfc'
import { expect, it, vi } from 'vitest'
import { defineComponent, h, reactive } from 'vue'
import EditContactModal from '@/components/EditContactModal.vue'
import ContactFieldRow from '@/components/ContactFieldRow.vue'
import { click, clientTemplate, fire, flush, ModalShell, nodes, renderView } from '../helpers/vue-node-renderer'

const { changeType } = vi.hoisted(() => ({ changeType: vi.fn() }))
vi.mock('@/components/modal', () => ({ popModal: vi.fn() }))
vi.mock('@/hooks/edit-contact', () => ({
  useEditContact: (data: object) => ({
    editItem: reactive({ ...data }),
    types: {},
    createTypeOptions: () => [],
    onTypeChanged: changeType,
    deleteField: (items: unknown[], index: number) => items.splice(index, 1),
  }),
}))

clientTemplate(EditContactModal, 'src/components/EditContactModal.vue')
clientTemplate(ContactFieldRow, 'src/components/ContactFieldRow.vue')

const control = (tag: string) => defineComponent({ setup: () => () => h(tag) })

it('uses one layout for every typed field and leaves no delete slot for the last phone number', async () => {
  const view = renderView(EditContactModal, {
    data: {
      phoneNumbers: [{ type: 'MOBILE', value: '5550100' }, { type: 'HOME', value: '5550101' }],
      emails: [{ type: 'HOME', value: 'Original email' }],
      addresses: [{ type: 'WORK', value: 'Fixture address' }],
      websites: [{ type: 'HOMEPAGE', value: 'https://example.com' }],
      ims: [{ protocol: 'CUSTOM', value: 'fixture' }],
    },
  }, {
    VModal: ModalShell,
    VSelect: control('select'),
    VTextField: control('input'),
    VDropdown: () => null,
    VFilledButton: () => h('button'),
    VOutlinedButton: () => h('button'),
  })
  const rows = () => nodes(view.root).filter(node => node.children.some(child => child.props.class === 'field-type'))
  const fields = () => rows().map(row => row.children.filter(child => child.tag !== '#comment'))
  try {
    expect(fields().map(row => row.map(child => child.tag))).toEqual(Array.from({ length: 6 }, () => ['div', 'input', 'button']))
    expect(fields().map(row => (row[1]?.props.label || row[1]?.props.placeholder))).toEqual(['telephone', 'telephone', 'email', 'address', 'website', 'im'])

    const email = fields()[2]!
    fire(email[1]!, 'onUpdate:modelValue', 'Updated email')
    const emailType = email[0]!.children.find(child => child.tag === 'select')!
    fire(emailType, 'onUpdate:modelValue', 'CUSTOM')
    fire(emailType, 'onChange', 'CUSTOM')
    await flush()
    expect(fields()[2]?.[1]?.props.modelValue).toBe('Updated email')
    expect(changeType).toHaveBeenLastCalledWith({ type: 'CUSTOM', value: 'Updated email' })

    click(fields()[1]![2]!)
    await flush()
    expect(fields().map(row => row.map(child => child.tag))).toEqual([['div', 'input'], ...Array.from({ length: 4 }, () => ['div', 'input', 'button'])])
    expect(fields()[0]?.[1]?.props.modelValue).toBe('5550100')
  } finally {
    view.unmount()
  }
})

it('keeps the shared type selector compact and lets the input use the remaining width', () => {
  const filename = 'src/components/ContactFieldRow.vue'
  const { descriptor } = parse(readFileSync(new URL(`../../${filename}`, import.meta.url), 'utf8'))
  const { code, errors } = compileStyle({
    filename, id: 'data-v-test', source: descriptor.styles[0]!.content, scoped: true,
  })
  expect(errors).toEqual([])
  expect(code).toMatch(/\.field-type\[data-v-test\]\s*\{[^}]*flex:\s*0 1 auto;[^}]*max-inline-size:/)
  expect(code).toMatch(/\.field-value\[data-v-test\]\s*\{[^}]*min-inline-size:\s*0;/)
})
