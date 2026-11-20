<template>
  <Teleport v-if="isActive" to="#header-end-slot" defer>
    <v-icon-button v-if="logPath" v-tooltip="$t('download')" @click="downloadLogs">
      <i-lucide:download />
    </v-icon-button>
    <v-icon-button v-if="lines.length > 0" v-tooltip="$t('developer.clear_logs')" :loading="clearing" @click="clearLogs">
      <i-lucide:trash-2 />
    </v-icon-button>
    <v-dropdown v-if="logPath" v-model="pathOpen">
      <template #trigger>
        <v-icon-button>
          <i-lucide:info />
        </v-icon-button>
      </template>
      <section class="card card-info">
        <div class="key-value vertical">
          <div class="key">{{ $t('path') }}</div>
          <div class="value">{{ logPath }}</div>
        </div>
      </section>
    </v-dropdown>
    <v-icon-button v-tooltip="$t('refresh')" :loading="loading" @click="reload">
      <i-material-symbols:refresh-rounded />
    </v-icon-button>
  </Teleport>
  <div class="log-viewer-wrap">
    <div v-if="loading && lines.length === 0" class="state-wrap">
      <v-circular-progress indeterminate />
    </div>
    <template v-else-if="lines.length > 0">
      <div ref="editorContainer" class="log-editor" />
      <div v-if="loadingMore" class="loading-more-bar">
        <v-circular-progress indeterminate class="sm" />
      </div>
    </template>
    <div v-else class="state-wrap">
      <i-lucide:scroll-text class="state-icon" />
      <span class="state-text">{{ $t('no_data') }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { showActionError } from '@/hooks/show-action-error'
import { ref, onMounted, onUnmounted, onActivated, onDeactivated, shallowRef, nextTick } from 'vue'
import { EditorView, lineNumbers } from '@codemirror/view'
import { EditorState } from '@codemirror/state'
import { syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language'
import { oneDark } from '@codemirror/theme-one-dark'
import emitter from '@/plugins/eventbus'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { useDownload } from '@/hooks/files'
import { initQuery, initLazyQuery, appLogsGQL, appLogPathGQL } from '@/lib/api/query'
import { initMutation, clearAppLogsGQL } from '@/lib/api/mutation'

const PAGE_SIZE = 200

const isActive = ref(false)
onActivated(() => { isActive.value = true })
onDeactivated(() => { isActive.value = false })

const lines = ref<string[]>([])
const offset = ref(0)
const hasMore = ref(true)
const loading = ref(false)
const loadingMore = ref(false)
const clearing = ref(false)
const editorContainer = ref<HTMLElement>()
const view = shallowRef<EditorView>()

const { urlTokenKey } = storeToRefs(useTempStore())
const { downloadFile } = useDownload(urlTokenKey)
const logPath = ref('')
const pathOpen = ref(false)

initQuery({
  handle(data: { appLogPath: string }, error: string) {
    if (!error && data?.appLogPath) logPath.value = data.appLogPath
  },
  document: appLogPathGQL,
})

let isDark = document.documentElement.classList.contains('dark')

const baseTheme = EditorView.theme({
  '&': { height: '100%', fontSize: '0.8125rem' },
  '.cm-scroller': { overflow: 'auto', fontFamily: "'SF Mono', 'Fira Code', 'Fira Mono', Menlo, Consolas, monospace" },
  '.cm-content': { padding: '8px 0' },
  '.cm-line': { padding: '0 12px' },
  '.cm-gutters': { backgroundColor: 'transparent', borderRight: 'none' },
  '.cm-activeLineGutter': { backgroundColor: 'transparent' },
  '&.cm-focused': { outline: 'none' },
})

const lightTheme = EditorView.theme({
  '.cm-activeLine': { backgroundColor: 'transparent' },
})

function buildExtensions() {
  return [
    lineNumbers(),
    syntaxHighlighting(defaultHighlightStyle),
    EditorView.lineWrapping,
    EditorState.readOnly.of(true),
    EditorView.editable.of(false),
    baseTheme,
    isDark ? oneDark : lightTheme,
  ]
}

function onScroll() {
  const scroller = view.value?.scrollDOM
  if (!scroller || loadingMore.value || !hasMore.value) return
  if (scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 300) {
    loadingMore.value = true
    fetchLogs()
  }
}

function setupScrollListener() {
  view.value?.scrollDOM.addEventListener('scroll', onScroll, { passive: true })
}

function teardownScrollListener() {
  view.value?.scrollDOM.removeEventListener('scroll', onScroll)
}

function createEditor(content: string) {
  if (!editorContainer.value) return
  teardownScrollListener()
  view.value?.destroy()
  view.value = new EditorView({
    state: EditorState.create({ doc: content, extensions: buildExtensions() }),
    parent: editorContainer.value,
  })
  setupScrollListener()
}

function appendToEditor(newContent: string) {
  const v = view.value
  if (!v) return
  v.dispatch({ changes: { from: v.state.doc.length, to: v.state.doc.length, insert: '\n' + newContent } })
}

const { fetch: fetchLogs } = initLazyQuery({
  handle(data: { appLogs: string[] }, error: string) {
    loading.value = false
    loadingMore.value = false
    if (error) return
    const incoming = data?.appLogs ?? []
    hasMore.value = incoming.length === PAGE_SIZE
    if (offset.value === 0) {
      lines.value = incoming
      nextTick(() => createEditor(incoming.join('\n')))
    } else {
      lines.value = lines.value.concat(incoming)
      appendToEditor(incoming.join('\n'))
    }
    offset.value += incoming.length
  },
  document: appLogsGQL,
  variables: () => ({ offset: offset.value, limit: PAGE_SIZE }),
})

function reload() {
  offset.value = 0
  lines.value = []
  hasMore.value = true
  loading.value = true
  fetchLogs()
}

const { mutate: clearMutate } = initMutation({ document: clearAppLogsGQL }, showActionError)

async function clearLogs() {
  clearing.value = true
  await clearMutate()
  clearing.value = false
  reload()
}

function downloadLogs() {
  if (logPath.value) {
    downloadFile(logPath.value, 'app.log')
  }
}

function colorModeChangedHandler() {
  isDark = document.documentElement.classList.contains('dark')
  if (view.value && lines.value.length > 0) {
    const doc = view.value.state.doc.toString()
    teardownScrollListener()
    view.value.destroy()
    view.value = new EditorView({
      state: EditorState.create({ doc, extensions: buildExtensions() }),
      parent: editorContainer.value!,
    })
    setupScrollListener()
  }
}

onMounted(() => {
  emitter.on('color_mode_changed', colorModeChangedHandler)
  reload()
})

onUnmounted(() => {
  teardownScrollListener()
  view.value?.destroy()
  emitter.off('color_mode_changed', colorModeChangedHandler)
})
</script>

<style lang="scss" scoped>
.log-viewer-wrap {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.log-editor {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.log-editor :deep(.cm-editor) {
  height: 100%;
}

.loading-more-bar {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 8px;
}

.state-wrap {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  flex: 1;
  color: var(--md-sys-color-on-surface-variant);

  .state-icon {
    width: 40px;
    height: 40px;
    opacity: 0.5;
  }

  .state-text {
    font-size: 0.875rem;
  }
}
</style>
