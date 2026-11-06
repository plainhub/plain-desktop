import { createApp, h, ref } from 'vue'
import VBreadcrumb from '@/components/base/VBreadcrumb.vue'
import tooltip from '@/plugins/tooltip'
import '@/styles/main.scss'

const initial = [
  { path: '/archive', name: 'Archive disk' },
  { path: '/archive/reports', name: 'Annual-project-reports-and-reference-documents-2026' },
  { path: '/archive/reports/photos', name: '完整的年度工作资料与归档照片文件夹' },
]

createApp({ setup() {
  const items = ref(initial)
  return () => h(VBreadcrumb, {
    items: items.value,
    label: 'Current path',
    wrap: true,
    onNavigate: (path: string) => { items.value = initial.slice(0, initial.findIndex(item => item.path === path) + 1) },
  })
} }).use(tooltip).mount('#app')
