import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { toast, requests } = vi.hoisted(() => ({ toast: vi.fn(), requests: [] as any[] }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: toast }))
vi.mock('@/lib/upload/upload', () => ({ getUploadUrl: () => '/upload' }))
vi.mock('@/lib/api/crypto', () => ({ chachaEncrypt: () => [], bitArrayToUint8Array: () => new Uint8Array(1) }))
vi.mock('@/lib/api/file', () => ({ tokenToKey: () => new Uint8Array(1) }))
vi.mock('@/lib/device/current', () => ({ getCurrentAuthToken: () => 'token' }))
vi.mock('@/lib/prefs', () => ({ get: () => 'client' }))

class Request {
  upload = { addEventListener: vi.fn() }
  readyState = 0
  status = 201
  responseText = ''
  onreadystatechange = () => {}
  onabort = () => {}
  open = vi.fn()
  setRequestHeader = vi.fn()
  send(form: FormData) {
    requests.push({ request: this, name: (form.get('file') as File).name })
  }
  abort() {
    this.onabort()
  }
  complete() {
    this.readyState = 4
    this.onreadystatechange()
  }
}
function packageFiles(): File[] {
  const files = ['vision.onnx', 'language.onnx', 'custom-tokenizer.json'].map((name) => new File(['123'], name))
  const manifest = {
    formatVersion: 1,
    files: files.map((file) => ({ name: file.name, size: file.size, sha256: '0'.repeat(64) })),
    image: { file: files[0].name },
    text: { file: files[1].name },
    tokenizer: { file: files[2].name },
  }
  return [new File([JSON.stringify(manifest)], 'manifest.json'), ...files]
}
beforeEach(() => {
  vi.resetModules()
  toast.mockClear()
  requests.length = 0
  vi.stubGlobal('XMLHttpRequest', Request)
})
afterEach(() => vi.unstubAllGlobals())
describe('model package uploads', () => {
  it('rejects incomplete selections before uploading any file', async () => {
    const { useAIModelUpload } = await import('@/hooks/ai-model-upload')
    const upload = useAIModelUpload()
    await upload.startUpload(packageFiles().slice(0, 2) as unknown as FileList, '/staged')
    expect(requests).toHaveLength(0)
    expect(toast).toHaveBeenCalledWith('ai.wrong_files', 'error')
    expect(upload.uploading.value).toBe(false)
  })
  it('supports custom filenames and uploads the manifest last', async () => {
    const { useAIModelUpload } = await import('@/hooks/ai-model-upload')
    const upload = useAIModelUpload()
    const finished = upload.startUpload(packageFiles() as unknown as FileList, '/staged')
    for (let i = 0; i < 4; i++) {
      await vi.waitFor(() => expect(requests.length).toBe(i + 1))
      requests[i].request.complete()
    }
    await finished
    expect(requests.map((item) => item.name)).toEqual(['vision.onnx', 'language.onnx', 'custom-tokenizer.json', 'manifest.json'])
    expect(upload.uploadDone.value).toBe(true)
  })
  it('cancelling stops subsequent files and does not activate a partial package', async () => {
    const { useAIModelUpload } = await import('@/hooks/ai-model-upload')
    const upload = useAIModelUpload()
    const finished = upload.startUpload(packageFiles() as unknown as FileList, '/staged')
    await vi.waitFor(() => expect(requests).toHaveLength(1))
    upload.cancelUpload()
    await finished
    expect(requests).toHaveLength(1)
    expect(upload.uploadDone.value).toBe(false)
    expect(upload.uploading.value).toBe(false)
  })
})
