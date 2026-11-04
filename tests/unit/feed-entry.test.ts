import { createSSRApp, ref } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { expect, it, vi } from 'vitest'
import { useFeedEntry } from '@/hooks/feed-entry'
import type { IFeedEntryDetail } from '@/lib/interfaces'

const callbacks = vi.hoisted(() => ({ query: null as any, sync: null as any }))
vi.mock('vue-router', () => ({ useRoute: () => ({ params: { id: 'entry' }, query: {} }) }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('pinia', () => ({ storeToRefs: (store: unknown) => store }))
vi.mock('@/stores/temp', () => ({ useTempStore: () => ({ app: ref({}), urlTokenKey: ref(null) }) }))
vi.mock('@/stores/main', () => ({ useMainStore: () => ({}) }))
vi.mock('@/hooks/markdown', () => ({ useSafeMarkdown: () => ({ render: async (text: string) => `<p>${text}</p>` }) }))
vi.mock('@/hooks/feeds', () => ({ useFeeds: () => ({ viewFeed: vi.fn() }) }))
vi.mock('@/plugins/router', () => ({ replacePath: vi.fn() }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/lib/api/query', () => ({
  feedEntryGQL: 'entry', tagsGQL: 'tags', initQuery: vi.fn(),
  initLazyQuery: ({ handle }: { handle: unknown }) => {
    callbacks.query = handle
    return { loading: ref(false), fetch: vi.fn() }
  },
}))
vi.mock('@/lib/api/mutation', () => ({
  saveFeedEntriesToNotesGQL: 'notes', syncFeedEntryContentGQL: 'sync',
  initMutation: ({ document }: { document: string }) => ({
    loading: ref(false), mutate: vi.fn(),
    onDone: (callback: unknown) => { if (document === 'sync') callbacks.sync = callback },
  }),
}))

it.each([
  ['Full article 中文', 'Short summary', 'Full article 中文'],
  ['', 'Updated summary 中文', 'Updated summary 中文'],
])('renders the synchronized entry with content %j and description %j', async (content, description, expected) => {
  let reader!: ReturnType<typeof useFeedEntry>
  const app = createSSRApp({
    setup() {
      reader = useFeedEntry()
      return () => null
    },
  })
  app.provide('isPhone', false)
  await renderToString(app)
  const original = { id: 'entry', content: '', description: 'Short summary' } as IFeedEntryDetail
  await callbacks.query({ feedEntry: original }, '')
  expect(reader.markdown.value).toBe('<p>Short summary</p>')
  const updated = { ...original, content, description }
  await callbacks.sync({ data: { syncFeedEntryContent: updated } })
  expect(reader.entry.value).toEqual(updated)
  expect(reader.markdown.value).toBe(`<p>${expected}</p>`)
})
