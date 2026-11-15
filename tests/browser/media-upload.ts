import { createApp, h, ref } from 'vue'
import { createI18n } from 'vue-i18n'
import MediaPageActions from '@/components/media/MediaPageActions.vue'
import tooltip from '@/plugins/tooltip'
import '@/styles/main.scss'

const path = '/fixtures/archive/' + 'long-folder-name-'.repeat(12)
const app = createApp({
  render: () => h(MediaPageActions, {
    placement: 'top',
    state: { filterTrash: false, checked: false },
    sort: { sortBy: '', items: [], onSort: () => {} },
    hideMoreMenu: true,
    upload: { dir: path, editable: true, onUploadFiles: () => {}, onUploadDir: () => {}, onEditDir: () => {} },
  }),
})
app.provide('isPhone', ref(false))
app.use(createI18n({ legacy: false, locale: 'en', messages: { en: {
  upload: 'Upload', upload_files: 'Upload files', upload_folder: 'Upload folder', save_to: 'Save to', edit: 'Edit',
} } })).use(tooltip).mount('#app')

document.querySelector<HTMLElement>('.v-dropdown-container button')!.click()
await new Promise(resolve => setTimeout(resolve, 50))
const menu = document.querySelector<HTMLElement>('.v-dropdown-portal')!
const destination = menu.querySelector<HTMLElement>('.upload-dest-path')!
const range = document.createRange()
range.selectNodeContents(destination)
const edit = menu.querySelector<HTMLElement>('.upload-dest-edit')!
const menuRect = menu.getBoundingClientRect()
document.body.dataset.layout = [menuRect.width, menuRect.left, menuRect.right, innerWidth, range.getClientRects().length, edit.getBoundingClientRect().right].join(',')
document.body.dataset.result = menuRect.width <= 322 &&
  menuRect.left >= 16 && menuRect.right <= innerWidth - 16 &&
  range.getClientRects().length > 1 && destination.textContent === path &&
  edit.getBoundingClientRect().right <= menuRect.right ? 'pass' : 'fail'
