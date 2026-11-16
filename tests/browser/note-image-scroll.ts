import { createApp, h, nextTick, onMounted, ref } from 'vue'
import { createPinia } from 'pinia'
import { EditorView } from '@codemirror/view'
import { undo } from '@codemirror/commands'
import NoteModeToggle from '@/views/notes/NoteModeToggle.vue'
import type { ViewMode } from '@/hooks/note-edit'
import MarkdownEditor from '@/components/MarkdownEditor.vue'
import '@/styles/main.scss'

const frame = () => new Promise<void>(resolve => requestAnimationFrame(() => resolve()))
const image = 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="600" height="400"><rect width="600" height="400" fill="#ccc"/></svg>')
const prefix = Array.from({ length: 50 }, (_, i) => `Paragraph ${i + 1}. Synthetic editor scroll fixture.\n`).join('\n')
const imageSource = `![Synthetic image](${image})`
const previous = new URLSearchParams(location.search).has('previous') ? imageSource + '\n\n' : ''
const imagePos = prefix.length + previous.length
const fixture = prefix + previous + imageSource + '\n\n' + (new URLSearchParams(location.search).has('middle') ? prefix : '')
const app = createApp({
  setup() {
    const content = ref(fixture)
    const result = ref('Running')
    const mode = ref<ViewMode>('editor')
    onMounted(async () => {
      try {
        const cm = document.querySelector<HTMLElement>('.cm-editor')!
        const view = EditorView.findFromDOM(cm)!
        if (previous) view.dispatch({ selection: { anchor: prefix.length } })
        view.dispatch({ effects: EditorView.scrollIntoView(imagePos, { y: 'start', yMargin: 80 }) })
        for (let n = 0; n < 12; n++) await frame()
        const image = cm.querySelector<HTMLElement>('.cm-md-img')!
        if (!image) throw new Error('Image not rendered')
        const before = image.getBoundingClientRect().top
        const scrollBefore = view.scrollDOM.scrollTop
        image.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))
        for (let n = 0; n < 12; n++) await frame()
        const after = view.coordsAtPos(imagePos)!.top
        const delta = after - before
        if (Math.abs(delta) > 2) throw new Error(`Clicked source moved ${delta}px; scroll ${scrollBefore} → ${view.scrollDOM.scrollTop}; source top ${before} → ${after}`)
        if (content.value !== fixture) throw new Error('Content changed on image click')
        if (view.state.selection.main.head !== imagePos) throw new Error('Cursor not on clicked image')
        view.dispatch({ changes: { from: imagePos + 2, insert: 'Edited ' } })
        if (!content.value.includes('![Edited Synthetic image]')) throw new Error('Image source cannot be edited')
        const selection = view.state.selection.main.head
        const edited = content.value
        document.querySelectorAll<HTMLButtonElement>('.toggle-btn')[1].click()
        await nextTick()
        for (let n = 0; n < 12; n++) await frame()
        if (mode.value !== 'source' || cm.querySelector('.cm-md-img, .cm-md-table, .cm-md-math')) throw new Error('Source mode still renders Markdown widgets')
        if (cm.querySelector('.cm-md-h1-line')) throw new Error('Source mode keeps rendered heading styles')
        if (!getComputedStyle(view.scrollDOM).fontFamily.includes('Mono')) throw new Error('Source font is not monospace')
        if (view.state.doc.toString() !== edited || view.state.selection.main.head !== selection) throw new Error('Switching to source lost content or cursor')
        if (Math.abs(view.coordsAtPos(imagePos)!.top - after) > 2) throw new Error('Switching source lost viewport position')
        view.dispatch({ selection: { anchor: 0 } })
        for (let n = 0; n < 4; n++) await frame()
        if (cm.querySelector('.cm-md-img')) throw new Error('Source mode renders inactive image')
        view.dispatch({ selection: { anchor: selection } })
        if (!undo(view)) throw new Error('Mode switch lost undo history')
        await nextTick()
        if (content.value !== fixture) throw new Error('Undo did not restore original source')
        document.querySelectorAll<HTMLButtonElement>('.toggle-btn')[0].click()
        await nextTick()
        if (EditorView.findFromDOM(cm) !== view) throw new Error('Mode switch replaced editor')
        result.value = `PASS: clicked source stays at ${after}px; viewport delta ${delta}px; content, cursor, editing, source mode and undo preserved`
      } catch (error) { result.value = `FAIL: ${String(error)}` }
    })
    return () => h('div', [h('div', { role: 'status' }, result.value), h(NoteModeToggle, { mode: mode.value, 'onUpdate:mode': value => { mode.value = value } }), h('div', { style: 'height:560px' }, [h(MarkdownEditor, { source: mode.value === 'source', modelValue: content.value, 'onUpdate:modelValue': value => { content.value = value } })])])
  },
})
app.use(createPinia())
app.config.globalProperties.$t = (key: string) => key
app.directive('tooltip', {})
app.mount('#app')
