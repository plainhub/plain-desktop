import { afterEach, expect, it, vi } from 'vitest'
import { h } from 'vue'
import AISearchSetupView from '@/components/ai/AISearchSetupView.vue'
import { ImageSearchStatusType } from '@/lib/status'
import { click, clientTemplate, content, flush, nodes, renderView } from '../helpers/vue-node-renderer'

const native = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@/lib/api/native-invoke', () => ({ invoke: native.invoke }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/ai/AISearchModelImport.vue', () => ({ default: { render: () => null } }))
vi.mock('@/hooks/ai/use-image-search-actions', () => ({
  useImageSearchActions: () => ({ enable: vi.fn(), enableLoading: false }),
}))
vi.mock('@/hooks/show-action-error', () => ({ showActionError: vi.fn() }))

clientTemplate(AISearchSetupView, 'src/components/ai/AISearchSetupView.vue')

afterEach(() => {
  vi.unstubAllGlobals()
  vi.clearAllMocks()
})

it('opens the Hugging Face model link through the Desktop system opener', async () => {
  vi.stubGlobal('__IS_TAURI__', true)
  native.invoke.mockResolvedValue(undefined)
  const view = renderView(AISearchSetupView, {
    status: { status: ImageSearchStatusType.UNAVAILABLE, modelSize: 785356945, modelDir: 'incoming', errorMessage: '' },
  }, {
    I18nT: { setup: (_props: unknown, { slots }: any) => () => h('p', slots.link?.()) },
    VFilledButton: { setup: (_props: unknown, { slots }: any) => () => h('button', slots.default?.()) },
  })
  try {
    const url = 'https://huggingface.co/google/siglip2-base-patch16-256'
    const link = nodes(view.root).find(node => node.tag === 'a' && node.props.href === url)
    expect(link, 'the translated model link renders as a clickable Vue anchor').toBeDefined()
    click(link!)
    await flush()
    await vi.waitFor(() => expect(native.invoke).toHaveBeenCalledExactlyOnceWith('plugin:opener|open_url', { url }))
    expect(nodes(view.root).some(node => node.tag === 'button' && content(node).includes('Google SigLIP 2 Base'))).toBe(true)
  } finally {
    view.unmount()
  }
})
