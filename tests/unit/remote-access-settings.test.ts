import { expect, it, vi } from 'vitest'
import { h, ref, type FunctionalComponent } from 'vue'
import VListItem from '@/components/base/VListItem.vue'
import AccessSettingsDialog from '@/views/home/remote-access/AccessSettingsDialog.vue'
import RemoteAccessCard from '@/views/home/remote-access/RemoteAccessCard.vue'
import { click, clientTemplate, content, fire, flush, ModalShell, nodes, renderView } from '../helpers/vue-node-renderer'

const mock = vi.hoisted(() => ({ local: true, modal: vi.fn(), close: vi.fn(), native: vi.fn(), invoke: vi.fn() }))
vi.mock('@/lib/api/native-invoke', () => ({ invoke: mock.invoke }))
vi.mock('@/lib/api/native-client', () => ({ nativeGraphql: mock.native }))
vi.mock('@/lib/device/local-mode', () => ({ isLocalMode: () => mock.local }))
vi.mock('@/components/modal', () => ({ openModal: mock.modal, popModal: mock.close }))
vi.mock('@/views/home/remote-access/use-remote-access', () => ({
  useRemoteAccess: () => ({ enabled: ref(true), hostname: ref('device.local') }),
}))
vi.mock('@/views/home/remote-access/use-http-server', () => ({
  useHttpServer: () => ({ httpAddresses: [], httpsAddresses: [], count: ref(0) }),
}))
vi.mock('@/views/home/remote-access/use-mdns', () => ({ useMdns: () => ({}) }))
vi.mock('@/views/home/remote-access/RemoteAccessAddresses.vue', async () => ({ default: { render: () => null } }))

const Button: FunctionalComponent = (_props, { attrs, slots }) => h('button', attrs, slots.default?.())

it.each([true, false])('offers Access Settings only when local mode is %s', (local) => {
  mock.local = local
  mock.modal.mockClear()
  const card = renderView(clientTemplate({ ...RemoteAccessCard }, 'src/views/home/remote-access/RemoteAccessCard.vue'), {}, {
    VTextButton: Button,
    VSwitch: Button, VIconButton: Button,
  })
  try {
    const footer = nodes(card.root).find((node) => node.tag === 'footer')!
    const settings = nodes(footer).find((node) => node.tag === 'button' && content(node) === 'access_settings')
    if (local) {
      expect(settings).toBeDefined()
      click(settings!)
      expect(mock.modal).toHaveBeenCalledOnce()
    } else {
      expect(settings).toBeUndefined()
      expect(mock.modal).not.toHaveBeenCalled()
    }
  } finally {
    card.unmount()
  }
})

const Field: FunctionalComponent<{ label: string; type: string; modelValue: string }> = (props, { emit }) =>
  h('input', { ...props, onInput: (value: string) => emit('update:modelValue', value) })
Field.props = { label: String, type: String, modelValue: String }

it.each([
  { password_type: 'NONE', configured: false, failsToEnable: false },
  { password_type: 'NONE', configured: true, failsToEnable: false },
  { password_type: 'FIXED', configured: true, failsToEnable: false },
  { password_type: 'NONE', configured: false, failsToEnable: true },
])('saves locally with $password_type (saved: $configured, enable failure: $failsToEnable)', async ({ password_type, configured, failsToEnable }) => {
  mock.local = true
  mock.native.mockReset()
  mock.invoke.mockReset().mockResolvedValue(configured)
  mock.native.mockResolvedValueOnce({ data: { prefs: { password_type, login_require_approval: true } } })
  if (failsToEnable) {
    mock.native.mockResolvedValueOnce({ data: { setPref: true } })
    mock.native.mockRejectedValueOnce(new Error('Unavailable'))
  }
  mock.native.mockResolvedValue({ data: { setPref: true } })
  const view = dialog()
  const button = (label: string) => nodes(view.root).find((node) => node.tag === 'button' && content(node) === label)!
  const field = (label: string) => nodes(view.root).find((node) => node.props.label === label)!
  try {
    await flush()
    expect(mock.native.mock.calls[0][0].variables).toEqual({ keys: ['password_type', 'login_require_approval'] })
    click(button(configured ? 'change_login_password' : 'set_login_password'))
    await flush()
    expect(field('new_login_password').props.type).toBe('password')
    fire(field('new_login_password'), 'onInput', 'synthetic-password')
    fire(field('confirm_login_password'), 'onInput', 'different')
    click(button('save'))
    await flush()
    expect(content(view.root)).toContain('login_password_mismatch')
    expect(mock.native).toHaveBeenCalledOnce()
    fire(field('confirm_login_password'), 'onInput', 'synthetic-password')
    click(button('save'))
    await flush()
    if (failsToEnable) {
      expect(content(view.root)).toContain('login_password_enable_failed')
      expect(field('new_login_password')).toBeUndefined()
      click(button('retry'))
      await flush()
    }
    const writes = mock.native.mock.calls.slice(1).map(([body]) => body.variables)
    expect(writes).toEqual([
      { key: 'password', value: 'synthetic-password' },
      ...(password_type === 'NONE' && !configured ? [{ key: 'password_type', value: 'FIXED' }] : []),
      ...(failsToEnable ? [{ key: 'password_type', value: 'FIXED' }] : []),
    ])
    expect(button('change_login_password')).toBeDefined()
    expect(field('new_login_password')).toBeUndefined()
    expect(mock.close).not.toHaveBeenCalled()
  } finally {
    view.unmount()
  }
})

const Switch: FunctionalComponent<{ checked: boolean; disabled: boolean; label: string }> = (props, { emit }) =>
  h('input', { ...props, onChange: (value: boolean) => emit('change', value) })
Switch.props = { checked: Boolean, disabled: Boolean, label: String }


function dialog(modal: object = ModalShell) {
  return renderView(
    clientTemplate({ ...AccessSettingsDialog }, 'src/views/home/remote-access/AccessSettingsDialog.vue'), {},
    { VModal: modal, VListItem: clientTemplate({ ...VListItem }, 'src/components/base/VListItem.vue'), VTextButton: Button, VFilledButton: Button, VTextField: Field, VSwitch: Switch, VIconButton: Button },
  )
}

it('renders a close button in the modal header and closes without saving', async () => {
  mock.local = true
  mock.close.mockClear()
  mock.native.mockReset().mockResolvedValue({ data: { prefs: { password_type: 'NONE', login_require_approval: true } } })
  mock.invoke.mockReset().mockResolvedValue(false)
  const modal = { setup(_: unknown, { slots }: { slots: Record<string, () => ReturnType<typeof h>[]> }) {
    return () => h('section', [h('header', slots.headline?.()), slots.content?.()])
  } }
  const view = dialog(modal)
  try {
    await flush()
    const header = nodes(view.root).find(node => node.tag === 'header')!
    const close = header.children.find(node => node.props['aria-label'] === 'close')
    expect(close?.tag).toBe('button')
    click(close!)
    expect(mock.close).toHaveBeenCalledOnce()
    expect(mock.native).toHaveBeenCalledOnce()
  } finally {
    view.unmount()
    mock.close.mockClear()
  }
})

it('shows the access switches and icons with only the confirmation hint', async () => {
  mock.local = true
  mock.native.mockReset().mockResolvedValue({ data: { prefs: { password_type: 'NONE', login_require_approval: true } } })
  mock.invoke.mockReset().mockResolvedValue(false)
  const view = dialog()
  try {
    await flush()
    const rows = nodes(view.root).filter(node => node.tag === 'li' &&
      nodes(node).some(child => child.tag === 'input'))
    expect(rows.map(content)).toEqual(['use_password_login', 'require_login_confirmationlogin_confirmation_hint'])
    const icons = rows.map(row => nodes(row).find(child => child.tag === 'svg'))
    expect(icons.map(icon => icon?.props['aria-hidden'])).toEqual(['true', 'true'])
    const shapes = icons.map(icon => icon!.children.find(node => node.tag === 'path')?.props.d)
    expect(shapes.every(Boolean)).toBe(true)
    expect(new Set(shapes).size).toBe(2)
  } finally {
    view.unmount()
  }
})

it.each([
  { local: false, mode: 'FIXED', initial: true, next: false, refused: false },
  { local: true, mode: 'NONE', initial: true, next: false, refused: false },
  { local: true, mode: 'NONE', initial: false, next: true, refused: false },
  { local: true, mode: 'FIXED', initial: true, next: false, refused: false },
  { local: true, mode: 'FIXED', initial: true, next: false, refused: true },
])('handles local=$local mode=$mode confirmation $initial→$next (refused=$refused)', async ({ local, mode, initial, next, refused }) => {
  mock.local = local
  mock.native.mockReset()
  mock.invoke.mockReset().mockResolvedValue(false)
  mock.native.mockImplementation(({ variables }) => variables.keys
    ? { data: { prefs: { password_type: mode, login_require_approval: initial } } }
    : { data: { setPref: !refused } })
  const view = dialog()
  const toggle = () => nodes(view.root).find((node) => node.props.label === 'require_login_confirmation')
  try {
    await flush()
    if (!local) {
      expect(mock.native).not.toHaveBeenCalled()
      expect(mock.invoke).not.toHaveBeenCalled()
      return
    }
    expect(toggle()?.props.checked).toBe(initial)
    expect(toggle()?.props.disabled).toBe(false)
    fire(toggle()!, 'onChange', next)
    await flush()
    const writes = mock.native.mock.calls.map(([body]) => body.variables).filter((vars) => vars.key)
    expect(writes).toEqual([{ key: 'login_require_approval', value: next }])
    expect(toggle()?.props.checked).toBe(!refused ? next : initial)
    expect(content(view.root).includes('login_confirmation_save_failed')).toBe(refused)
  } finally {
    view.unmount()
  }
})

it('opens the password editor from a disabled password-login switch without writing', async () => {
  mock.local = true
  mock.native.mockReset()
  mock.invoke.mockReset().mockResolvedValue(false)
  mock.native.mockResolvedValueOnce({ data: { prefs: { password_type: 'NONE', login_require_approval: true } } })
  const view = dialog()
  const find = (label: string) => nodes(view.root).find((node) => node.props.label === label)
  try {
    await flush()
    expect(find('use_password_login')?.props.checked).toBe(false)
    fire(find('use_password_login')!, 'onChange', true)
    await flush()
    expect(find('new_login_password')?.props.type).toBe('password')
    expect(find('confirm_login_password')?.props.type).toBe('password')
    expect(mock.native).toHaveBeenCalledOnce()
  } finally {
    view.unmount()
  }
})

it.each([true, false])('writes only NONE when disabling password login (accepted: %s)', async (accepted) => {
  mock.local = true
  mock.native.mockReset()
  mock.invoke.mockReset().mockResolvedValue(false)
  mock.native.mockResolvedValueOnce({ data: { prefs: { password_type: 'FIXED', login_require_approval: false } } })
  mock.native.mockResolvedValueOnce({ data: { setPref: accepted } })
  const view = dialog()
  const find = (label: string) => nodes(view.root).find((node) => node.props.label === label)
  try {
    await flush()
    expect(find('use_password_login')?.props.checked).toBe(true)
    fire(find('use_password_login')!, 'onChange', false)
    await flush()
    expect(find('use_password_login')?.props.checked).toBe(!accepted)
    expect(find('require_login_confirmation')?.props.checked).toBe(false)
    expect(mock.native.mock.calls.slice(1).map(([request]) => request.variables)).toEqual([{ key: 'password_type', value: 'NONE' }])
    expect(content(view.root).includes('login_password_disable_failed')).toBe(!accepted)
  } finally {
    view.unmount()
  }
})


it('re-enables an already saved password when reopened without showing the editor', async () => {
  mock.local = true
  mock.native.mockReset()
  mock.invoke.mockReset().mockResolvedValue(true)
  mock.native.mockResolvedValueOnce({ data: { prefs: { password_type: 'NONE', login_require_approval: true } } })
  mock.native.mockResolvedValueOnce({ data: { setPref: true } })
  const view = dialog()
  const find = (label: string) => nodes(view.root).find((node) => node.props.label === label)
  try {
    await flush()
    expect(mock.invoke).toHaveBeenCalledWith('native_password_configured')
    expect(content(view.root)).toContain('change_login_password')
    fire(find('use_password_login')!, 'onChange', true)
    await flush()
    expect(find('new_login_password')).toBeUndefined()
    expect(find('use_password_login')?.props.checked).toBe(true)
    expect(mock.native.mock.calls.slice(1).map(([request]) => request.variables)).toEqual([{ key: 'password_type', value: 'FIXED' }])
  } finally {
    view.unmount()
  }
})
