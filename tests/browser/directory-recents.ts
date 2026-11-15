import { createApp, h, nextTick, ref } from 'vue'
import { createI18n } from 'vue-i18n'
import DirectoryBrowser from '@/components/DirectoryBrowser.vue'
import type { IStorageMount } from '@/lib/interfaces'
import files from '@/locales/zh-CN/files'
import '@/styles/main.scss'

const disk = (letter: string, name: string, freeBytes: number, totalBytes: number) =>
  ({ id: letter, name, mountPoint: `${letter}:\\`, freeBytes, totalBytes, driveType: 'INTERNAL_STORAGE' }) as IStorageMount

const selected = ref('D:/Projects/Photos')
createApp({
  render: () => h(DirectoryBrowser, {
    volumes: [disk('C', '系统', 141e9, 501e9), disk('D', '资料', 413e9, 501e9), disk('E', '备份', 2e12, 4e12)],
    activeRoot: selected.value.slice(0, 3),
    currentDir: selected.value,
    listing: false,
    dirItems: [{ path: `${selected.value}/Exports`, isSystem: false }],
    recentDirs: ['D:/Projects/Photos', 'E:/Backup/Photos', 'C:/Users/Demo/Downloads'],
    dirName: path => path.split('/').pop()!,
    onSelectRecent: path => { selected.value = path },
  }),
}).use(createI18n({ legacy: false, locale: 'zh-CN', messages: { 'zh-CN': files }, missingWarn: false, fallbackWarn: false })).mount('#app')

const subtitle = document.querySelector<HTMLElement>('.volume-item.active .subtitle')!
const compact = subtitle.textContent === '可用 413 GB / 501 GB' && subtitle.scrollWidth <= subtitle.clientWidth
document.querySelectorAll<HTMLElement>('.recent-item')[1].click()
await nextTick()
document.body.dataset.result = compact && document.querySelector('.selection')?.textContent?.includes('备份 (E:)') ? 'pass' : 'fail'
