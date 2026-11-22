import { nextTick, reactive } from 'vue'
import { afterEach, expect, it, vi } from 'vitest'
import { useTokenSearch } from '@/hooks/token-search'
import { valueOptionsForGroup } from '@/components/header-search/options'
import { extractTokensAndText } from '@/lib/token-search-dom'
import apps from '@/locales/zh-CN/apps'
import contacts from '@/locales/zh-CN/contacts'
import messages from '@/locales/zh-CN/messages'
import { renderView } from '../helpers/vue-node-renderer'

const { t } = vi.hoisted(() => ({ t: vi.fn<(key: string) => string>() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t }) }))

class Element {
  nodeType = 1
  className = ''
  dataset: Record<string, string> = {}
  textContent = ''
  childNodes: Element[] = []
  focus() {}
  addEventListener() {}
  setAttribute() {}
  appendChild(child: Element) { this.childNodes.push(child) }
  set innerHTML(_: string) { this.childNodes = [] }
  querySelector(selector: string) {
    return this.childNodes.find(child => '.' + child.className === selector)
  }
}

afterEach(() => vi.unstubAllGlobals())

it('uses current option labels across pages, without changing the raw search value or replacing a focused token', async () => {
  t.mockImplementation(key => {
    const [group, value] = key.split('.')
    const labels: Record<string, Record<string, string>> = { app_type: apps.app_type, call_type: contacts.call_type, message_type: messages.message_type }
    return labels[group]?.[value] ?? key
  })
  vi.stubGlobal('Node', { ELEMENT_NODE: 1 })
  vi.stubGlobal('document', Object.assign(new EventTarget(), {
    createElement: () => new Element(),
    createTextNode: (textContent: string) => ({ nodeType: 3, textContent }),
  }))
  const editable = new Element() as unknown as HTMLDivElement
  const props = reactive<Parameters<typeof useTokenSearch>[0]>({
    text: '', tokens: [], enterSubmits: true, keyOptions: [], valueOptions: {},
  })
  let search!: ReturnType<typeof useTokenSearch>
  const view = renderView({ setup() {
    search = useTokenSearch(props, vi.fn())
    search.editableRef.value = editable
    return () => null
  } })
  const label = () => editable.querySelector('.token')?.querySelector('.token-value')?.textContent
  const options = (group: string) => valueOptionsForGroup({
    group, t, history: [], mediaTags: [], mediaBuckets: [], feeds: [],
  })
  try {
    for (const [group, value, expected] of [
      ['messages', 'INBOX', '收件箱'], ['calls', 'MISSED', '未接'], ['apps', 'SYSTEM', '系统'],
    ]) {
      props.valueOptions = options(group)
      props.tokens = [{ key: 'type', value }]
      await nextTick()
      await nextTick()
      expect(label()).toBe(expected)
      expect(extractTokensAndText(editable).tokens).toEqual([{ key: 'type', value }])
    }

    props.tokens = [{ key: 'feed_id', value: 'feed-1' }]
    props.valueOptions = options('feeds')
    await nextTick()
    await nextTick()
    expect(label()).toBe('feed-1')
    search.onFocus()
    const existing = editable.querySelector('.token')
    props.valueOptions = { feed_id: [{ value: 'feed-1', label: 'Fixture feed' }] }
    await nextTick()
    expect(label()).toBe('Fixture feed')
    expect(editable.querySelector('.token')).toBe(existing)
    expect(extractTokensAndText(editable).tokens).toEqual([{ key: 'feed_id', value: 'feed-1' }])
  } finally { view.unmount() }
})
