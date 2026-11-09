import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createRenderer, nextTick, ref, ssrContextKey, type Ref } from 'vue'
import type { IFeedEntry } from '@/lib/interfaces'

const fixture = vi.hoisted(() => ({
  items: undefined as unknown as Ref<IFeedEntry[]>,
  resolve: vi.fn(), close: vi.fn(),
}))
vi.mock('@/lib/format', () => ({ formatTimeAgo: vi.fn(), formatDateTime: vi.fn() }))
vi.mock('@/lib/api/file', () => ({ resolveFileUrlByPath: fixture.resolve }))
vi.mock('@/hooks/native-resource-url', () => ({
  closeNativeResources: fixture.close,
  isExternalReference: (path: string) => path.startsWith('https://'),
  isNativeResourceUrl: (url: string) => url.startsWith('plain-resource://'),
}))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ urlTokenKey: 'fixture-key' }) }))
vi.mock('@/views/feeds/hooks/useFeedEntriesData', () => ({ useFeedEntriesData: () => ({ items: fixture.items }) }))
vi.mock('@/views/feeds/hooks/useFeedEntriesActions', () => ({ useFeedEntriesActions: () => ({}) }))
vi.mock('@/components/virtualscroll', () => ({ default: {} }))
import FeedsSidebar2 from '@/views/feeds/FeedsSidebar2.vue'
FeedsSidebar2.render = () => null

const renderer = createRenderer<object, object>({
  insert: vi.fn(), remove: vi.fn(), createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
  setText: vi.fn(), setElementText: vi.fn(), parentNode: () => null, nextSibling: () => null, patchProp: vi.fn(),
})
let unmount: (() => void) | undefined
function mount() {
  const app = renderer.createApp(FeedsSidebar2)
  app.provide(ssrContextKey, { modules: new Set() })
  app.provide('isPhone', false)
  app.mount({})
  unmount = () => app.unmount()
}
async function flush() {
  for (let i = 0; i < 12; i++) await nextTick()
}
function setImages(...paths: string[]) {
  fixture.items.value = paths.map((image, index) => ({ id: String(index), image }) as IFeedEntry)
}
function delayedImages() {
  const pending = new Map<string, (url: string) => void>()
  fixture.resolve.mockImplementation((_key: string, path: string) => new Promise<string>((resolve) => pending.set(path, resolve)))
  return (path: string) => pending.get(path)!(`plain-resource://file/${path}`)
}
const closed = () => fixture.close.mock.calls.flatMap(([paths]) => paths as string[])

beforeEach(() => {
  vi.resetAllMocks()
  fixture.items = ref([])
  fixture.close.mockResolvedValue(undefined)
  fixture.resolve.mockImplementation(async (_key: string, path: string) => `plain-resource://file/${path}`)
  vi.spyOn(console, 'warn').mockImplementation(() => {})
})
afterEach(() => {
  unmount?.(); unmount = undefined
  vi.restoreAllMocks()
})

it('closes pending thumbnail opens on unmount and closes their late results', async () => {
  const finish = delayedImages()
  setImages('first', 'second')
  mount()
  expect(fixture.resolve).toHaveBeenCalledTimes(2)
  unmount?.(); unmount = undefined
  expect(closed()).toEqual(['first', 'second'])
  finish('first'); finish('second')
  await flush()
  expect(closed()).toEqual(['first', 'second', 'first', 'second'])
})

it('releases replaced pending images while retaining references shared with the latest list', async () => {
  const pending: { path: string, finish: (url: string) => void }[] = []
  fixture.resolve.mockImplementation((_key: string, path: string) => new Promise<string>((finish) => pending.push({ path, finish })))
  setImages('old', 'shared'); mount()
  setImages('new', 'shared'); await nextTick()
  expect(closed()).toEqual(['old'])
  pending.slice(2).forEach(({ path, finish }) => finish(`plain-resource://file/${path}`))
  await flush()
  pending.slice(0, 2).forEach(({ path, finish }) => finish(`plain-resource://file/${path}`))
  await flush()
  expect(closed()).toEqual(['old', 'old'])
  unmount?.(); unmount = undefined
  expect(closed().sort()).toEqual(['new', 'old', 'old', 'shared'])
})

it('preserves external image URLs and cleans successful opens even if a sibling fails', async () => {
  fixture.resolve.mockImplementation(async (_key: string, path: string) => {
    if (path === 'broken') throw new Error('unavailable')
    return `plain-resource://file/${path}`
  })
  setImages('https://feed.invalid/image.png', 'image', 'broken'); mount()
  await flush()
  expect(fixture.resolve).toHaveBeenCalledTimes(2)
  expect(console.warn).toHaveBeenCalledWith('[FeedsSidebar2] Failed to load image', expect.any(Error))
  unmount?.(); unmount = undefined
  expect(closed()).toEqual(['image'])
})
