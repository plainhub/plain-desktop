import { expect, it, vi } from 'vitest'
import { h, ref, type SetupContext } from 'vue'
import ImageSearchButton from '@/components/ai/ImageSearchButton.vue'
import AIImageSearchModal from '@/components/ai/AIImageSearchModal.vue'
import AISearchActiveView from '@/components/ai/AISearchActiveView.vue'
import AISearchSetupView from '@/components/ai/AISearchSetupView.vue'
import AISearchModelImport from '@/components/ai/AISearchModelImport.vue'
import type { IImageSearchStatus } from '@/lib/interfaces'
import { ImageSearchStatusType } from '@/lib/status'
import { click, clientTemplate, content, flush, ModalShell, nodes, renderView } from '../helpers/vue-node-renderer'

const status = ref<IImageSearchStatus>()
vi.mock('@/hooks/ai/use-image-search-status', () => ({ useImageSearchStatus: () => ({ status }) }))
vi.mock('@/hooks/ai/use-image-search-actions', () => ({ useImageSearchActions: () => ({}) }))
vi.mock('@/hooks/ai-model-upload', () => ({ useAIModelUpload: () => ({ uploading: false, uploadDone: ref(false) }) }))
vi.mock('@/hooks/show-action-error', () => ({ showActionError: vi.fn() }))
vi.mock('@/lib/browser', () => ({ openUrl: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))

clientTemplate(ImageSearchButton, 'src/components/ai/ImageSearchButton.vue')
clientTemplate(AIImageSearchModal, 'src/components/ai/AIImageSearchModal.vue')
clientTemplate(AISearchActiveView, 'src/components/ai/AISearchActiveView.vue')
clientTemplate(AISearchSetupView, 'src/components/ai/AISearchSetupView.vue')
clientTemplate(AISearchModelImport, 'src/components/ai/AISearchModelImport.vue')

const Button = { setup: (_: unknown, { slots }: SetupContext) => () => h('button', slots.default?.()) }
const Empty = { render: () => null }
const DangerAction = { props: ['label'], setup: (props: { label: string }) => () => h('button', props.label) }

it('removes the installation actions when the model becomes ready even if scanning skipped images', async () => {
  status.value = {
    status: ImageSearchStatusType.UNAVAILABLE,
    downloadProgress: 0,
    errorMessage: '',
    modelSize: 1024,
    modelDir: 'incoming',
    isIndexing: false,
    totalImages: 0,
    indexedImages: 0,
  }
  const view = renderView(ImageSearchButton, {}, {
    VModal: ModalShell,
    VIconButton: Button,
    VOutlinedButton: Button,
    VFilledButton: Button,
    DangerAction,
    I18nT: Empty,
    VCircularProgress: Empty,
    ProgressCard: Empty,
  })
  try {
    click(nodes(view.root).find(node => node.tag === 'button')!)
    await flush()
    const buttonLabels = () => nodes(view.root).filter(node => node.tag === 'button').map(content)
    const download = 'ai.activate_download · Google SigLIP 2 Base (256)'
    expect(buttonLabels()).toEqual(expect.arrayContaining([download, 'ai.manual_upload']))

    status.value = {
      ...status.value,
      status: ImageSearchStatusType.READY,
      totalImages: 10,
      indexedImages: 7,
      errorMessage: '3 images could not be decoded',
    }
    await flush()
    expect(buttonLabels()).not.toContain(download)
    expect(buttonLabels()).not.toContain('ai.manual_upload')
    expect(buttonLabels()).toEqual(expect.arrayContaining(['ai.rescan', 'ai.unload_model']))
    expect(content(view.root)).toContain('3 images could not be decoded')

    status.value = { ...status.value, status: ImageSearchStatusType.UNAVAILABLE, totalImages: 0, indexedImages: 0, errorMessage: '' }
    await flush()
    expect(buttonLabels()).toEqual(expect.arrayContaining([download, 'ai.manual_upload']))
  } finally {
    view.unmount()
  }
})
