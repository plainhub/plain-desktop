import { createSSRApp } from 'vue'
import { renderToString } from '@vue/server-renderer'
import { createI18n } from 'vue-i18n'
import { expect, it, vi } from 'vitest'
import UploadBatchTaskItem from '@/components/UploadBatchTaskItem.vue'
import { registerUploadItems, removeUploadBatch } from '@/lib/upload/batch-progress'
import { addUploadTaskAndWait, pauseUploadsByBatch, removeUploadsByBatch, resumeUploadsByBatch } from '@/lib/upload/upload-queue'
import zhCN from '@/locales/zh-CN'
import type { IUploadItem } from '@/stores/temp'

const { upload } = vi.hoisted(() => ({ upload: vi.fn() }))
vi.mock('@/lib/upload/upload', () => ({ upload, discardUploadChunks: vi.fn() }))
vi.mock('@/lib/upload/context', () => ({ getUploadContext: vi.fn(), cancelUploadOperations: vi.fn() }))
vi.mock('@/plugins/eventbus', () => ({ default: { emit: vi.fn() } }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ uploads: [] }) }))

const batchId = 'batch'

function makeItem(id: string): IUploadItem {
  const file = new File(['fixture'], `${id}.txt`)
  return { id, batchId, file, fileName: file.name, dir: '/fixtures', status: 'created', uploadedSize: 0, error: '' }
}

function renderBatch() {
  const app = createSSRApp(UploadBatchTaskItem, { batchId })
  app.use(createI18n({ legacy: false, locale: 'zh-CN', messages: { 'zh-CN': zhCN } }))
  app.component('VIconButton', { template: '<button><slot /></button>' })
  app.directive('tooltip', {})
  return renderToString(app)
}

it('shows a partially completed upload batch as paused, then completes on resume', async () => {
  vi.stubGlobal('window', new EventTarget())
  const first = makeItem('first')
  const second = makeItem('second')
  const interrupted = Promise.withResolvers<void>()
  upload.mockResolvedValueOnce(undefined).mockReturnValueOnce(interrupted.promise).mockResolvedValueOnce(undefined)
  registerUploadItems([first, second])

  try {
    const firstDone = addUploadTaskAndWait(first, false)
    const secondDone = addUploadTaskAndWait(second, false)
    void secondDone.catch(() => undefined)
    await firstDone
    expect(await renderBatch()).toContain('上传中')

    expect(pauseUploadsByBatch(batchId)).toEqual([second])
    interrupted.resolve()
    const pausedCard = await renderBatch()
    expect(pausedCard).toContain('resume-btn')
    expect(pausedCard).toContain('已暂停')

    resumeUploadsByBatch(batchId)
    await secondDone
    const completedCard = await renderBatch()
    expect(completedCard).toContain('已上传')
    expect(completedCard).not.toContain('resume-btn')
    expect(upload.mock.calls.map(([item]) => item.id)).toEqual([first.id, second.id, second.id])
  } finally {
    interrupted.resolve()
    removeUploadsByBatch(batchId)
    removeUploadBatch(batchId)
    vi.unstubAllGlobals()
  }
})
