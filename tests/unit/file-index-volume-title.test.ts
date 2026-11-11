import { readFileSync } from 'node:fs'
import { expect, it, vi } from 'vitest'
import { createSSRApp, h, ref } from 'vue'
import { renderToString } from 'vue/server-renderer'
import type { IStorageMount } from '@/lib/interfaces'
import type { FileIndexVolume } from '@/lib/api/file-index'
import { homeStatsGQL } from '@/lib/api/home-stats'
import { extractDocuments } from '@/lib/api/graphql/extract'
import { parseDocument, type Selection } from '@/lib/api/graphql/parser'
import { getStorageVolumeTitle } from '@/lib/volumes'

const state = vi.hoisted(() => ({ volumes: [] as FileIndexVolume[] }))
vi.mock('@/lib/prefs/appearance', () => ({ get: (_key: string, fallback: unknown) => fallback }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/lib/api/gql-client', () => ({ bindGraphqlClient: vi.fn() }))
vi.mock('@/lib/api/query', () => ({ initQuery: ({ handle }: any) => {
  handle({ fileIndexVolumes: state.volumes, fileIndexDefaults: {} }, '')
  return { loading: ref(false), refetch: vi.fn() }
} }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: { subscribe: () => vi.fn() } }))
vi.mock('@/components/modal', () => ({ pushModal: vi.fn() }))
vi.mock('@/views/home/file-index/FileIndexScopeDialog.vue', () => ({ default: {} }))
vi.mock('@/views/home/home', () => ({ useScanAction: () => ({
  scanProgress: ref({ indexed: 0, total: 0 }), scanActive: ref(false), percent: ref(0), stateLabel: ref(''),
  showPause: ref(false), showResume: ref(false), showStop: ref(false), showRebuild: ref(true),
  rebuildIndexLoading: ref(false), rebuildVolumeLoading: ref(false),
  pauseScan: vi.fn(), resumeScan: vi.fn(), stopScan: vi.fn(), rebuildIndex: vi.fn(), rebuildVolume: vi.fn(),
}) }))
import FileIndexCard from '@/views/home/file-index/FileIndexCard.vue'

const files = new Map(['query', 'fragments'].map((name) => [
  `lib/api/${name}.ts`, readFileSync(new URL(`../../src/lib/api/${name}.ts`, import.meta.url), 'utf8'),
]))
const mountsQuery = extractDocuments(files).docs.find((document) => document.key === 'mountsGQL')!.text
const t = (key: string) => key

function selectMount(query: string, mount: IStorageMount): IStorageMount {
  const document = parseDocument(query)
  const selection = document.operations[0].selections.find((item) => item.kind === 'field' && item.name === 'mounts')!
  function fields(selections: Selection[]): string[] {
    return selections.flatMap((item) => {
      if (item.kind === 'field') return [item.name]
      if (item.kind === 'inline') return fields(item.selections)
      return fields(document.fragments.find((fragment) => fragment.name === item.name)!.selections)
    })
  }
  if (selection.kind !== 'field') throw new Error('Missing mounts selection')
  return Object.fromEntries(fields(selection.selections).map((key) => [key, mount[key as keyof IStorageMount]])) as unknown as IStorageMount
}

const base = {
  id: 'external', name: 'usb1', alias: '', label: '', path: '/media/archive', mountPoint: '/media/archive',
  driveType: 'USB_STORAGE', fsType: 'ext4', diskId: 'disk-a', diskModel: 'Example Drive',
  totalBytes: 4000e9, freeBytes: 3500e9, usedBytes: 500e9, remote: false,
} as IStorageMount

async function render(mounts: IStorageMount[]) {
  const app = createSSRApp(FileIndexCard, { mounts })
  app.config.globalProperties.$t = t
  for (const name of ['VIconButton', 'VTextButton']) {
    app.component(name, { render() { return h('button', this.$slots.default?.()) } })
  }
  app.directive('tooltip', {})
  return renderToString(app)
}

it.each([
  ['unlabelled USB', {}, 'Example Drive (4.0 TB)'],
  ['volume label', { label: 'Photos' }, 'Photos'],
  ['user alias', { label: 'Photos', alias: 'Archive' }, 'Archive'],
  ['named system volume', { mountPoint: '/', name: 'System Drive', driveType: 'INTERNAL_STORAGE' }, 'System Drive'],
  ['unnamed system volume', { mountPoint: '/', name: '/', driveType: 'INTERNAL_STORAGE' }, 'internal_storage'],
  ['platform volume name', { diskModel: '', name: 'External Media' }, 'External Media'],
  ['zero-capacity drive', { totalBytes: 0 }, 'Example Drive'],
  ['EFI partition', { fsType: 'vfat', totalBytes: 500e6 }, 'efi_partition'],
  ['SD card', { name: '', diskModel: '', driveType: 'SDCARD' }, 'sdcard'],
  ['app storage', { name: '', diskModel: '', driveType: 'APP' }, 'app_data'],
] as const)('renders the same %s title from Home and FilesSidebar query data', async (_name, overrides, expected) => {
  const mount = { ...base, ...overrides } as IStorageMount
  state.volumes = [{ id: mount.id, path: mount.mountPoint, isAvailable: true, indexMode: 'UNCONFIGURED', includedDirs: [], excludedDirs: [] }]
  const homeMount = selectMount(homeStatsGQL(), mount)
  const sidebarMount = selectMount(mountsQuery, mount)
  expect(getStorageVolumeTitle(sidebarMount, t)).toBe(expected)
  const html = await render([homeMount])
  expect(html).toContain(`>${expected}</h3>`)
  expect(getStorageVolumeTitle(homeMount, t)).toBe(getStorageVolumeTitle(sidebarMount, t))
})

it('retains offline volumes and uses the path when mount metadata is absent', async () => {
  state.volumes = [{ id: 'missing', path: '/media/offline', isAvailable: false, indexMode: 'BLOCKLIST', includedDirs: [], excludedDirs: [] }]
  const html = await render([])
  expect(html).toContain('>/media/offline · offline</h3>')
  expect(html).toContain('unavailable')
  expect(html).toContain('disabled')
})

it('requests the same complete mount data for both surfaces', () => {
  expect(selectMount(homeStatsGQL(), base)).toEqual(selectMount(mountsQuery, base))
})
