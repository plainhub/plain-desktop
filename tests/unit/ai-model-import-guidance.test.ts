import { expect, it, vi } from 'vitest'
import { h, ref } from 'vue'
import AISearchModelImport from '@/components/ai/AISearchModelImport.vue'
import { clientTemplate, content, nodes, renderView } from '../helpers/vue-node-renderer'

vi.mock('@/hooks/ai-model-upload', () => ({
  useAIModelUpload: () => ({ uploading: false, uploadDone: ref(false) }),
}))
vi.mock('@/hooks/ai/use-image-search-actions', () => ({
  useImageSearchActions: () => ({ enableLoading: false }),
}))

clientTemplate(AISearchModelImport, 'src/components/ai/AISearchModelImport.vue')

it('renders the required model files below the manual upload button', () => {
  const view = renderView(AISearchModelImport, { modelDir: 'incoming' }, {
    ProgressCard: { render: () => null },
    VOutlinedButton: { setup: (_: unknown, { slots }: any) => () => h('button', slots.default?.()) },
    I18nT: { setup: (_: unknown, { slots }: any) => () => h('p', [slots.files?.(), slots.manifest?.()]) },
  })
  try {
    const elements = nodes(view.root)
    const button = elements.findIndex(node => node.tag === 'button' && content(node) === 'ai.manual_upload')
    const guidance = elements.findIndex(node => node.tag === 'p' && node.props.keypath === 'ai.manual_upload_hint')
    expect(button).toBeGreaterThanOrEqual(0)
    expect(guidance).toBeGreaterThan(button)
    for (const file of ['manifest.json', 'image.onnx', 'text.onnx', 'tokenizer.json']) {
      expect(content(elements[guidance])).toContain(file)
    }
  } finally {
    view.unmount()
  }
})
