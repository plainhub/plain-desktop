<!-- eslint-disable vue/no-v-html -->
<template>
  <div class="page" :class="{ fullscreen: isFullscreen }">
    <header
      class="topbar"
      :class="{ 'topbar--tauri': isTauri }"
      :data-tauri-drag-region="isTauri ? 'deep' : null"
    >
      <div class="file-chip">
        <div class="file-icon">
          <i-lucide-file-text />
        </div>
        <div class="title-wrap">
          <div class="title-line">
            <div class="title">{{ displayTitle }}</div>
            <span class="type-badge" :class="{ plain: kind === 'txt' || kind === 'doc' }">{{ kindLabel }}</span>
          </div>
          <div class="meta">
            <span v-if="lastModified" v-tooltip="formatDateTime(lastModified)">{{ formatTimeAgo(lastModified) }}</span>
          </div>
        </div>
      </div>

      <div v-if="statusText" class="status" :class="{ saving: saving }">
        <v-circular-progress v-if="saving" indeterminate :size="16" :width="2" />
        <i-material-symbols:check-circle-rounded v-else class="status-icon" />
        <span class="status-text">{{ statusText }}</span>
      </div>

      <div class="actions">
        <template v-if="isEditing">
          <v-outlined-button class="action-btn" :loading="saving" :disabled="saving || !dirty" @click="save">
            {{ $t('save') }}
          </v-outlined-button>
          <v-outlined-button class="action-btn" @click="openViewer">
            {{ $t('view') }}
          </v-outlined-button>
        </template>
        <template v-else>
          <v-outlined-button
            v-if="showInFinder"
            v-tooltip="$t('show_in_finder')"
            class="action-btn"
            :aria-label="$t('show_in_finder')"
            @click="revealInFinder"
          >
            <i-lucide-folder-open />
          </v-outlined-button>
          <v-outlined-button
            v-else
            v-tooltip="$t('download')"
            class="action-btn"
            :aria-label="$t('download')"
            @click="downloadFile"
          >
            <i-lucide-download />
          </v-outlined-button>
          <v-outlined-button
            v-if="canEdit"
            v-tooltip="$t('edit')"
            class="action-btn"
            :aria-label="$t('edit')"
            @click="openEditor"
          >
            <i-lucide-pencil />
          </v-outlined-button>
          <header-actions :logged-in="isLoggedIn" />
        </template>
      </div>
    </header>

    <main v-if="isEditing" class="editor">
      <CodeEditor v-model="draft" :language="language" />
    </main>

    <template v-else>
      <ViewerToolbar
        v-if="!loading && !error"
        :kind="kind"
        :structured="structured"
        :mode="mode"
        :depth="depth"
        :path-open="pathOpen"
        :wrap="wrap"
        :indent-size="indentSize"
        :big="big"
        :can-transform="canTransform"
        :fullscreen="isFullscreen"
        @update:mode="mode = $event"
        @update:depth="depth = $event"
        @update:path-open="pathOpen = $event"
        @update:wrap="wrap = $event"
        @update:indent-size="indentSize = $event as 2 | 4"
        @toggle-fullscreen="isFullscreen = !isFullscreen"
        @format="format"
        @minify="minify"
      />

      <JsonPathBar
        v-if="kind === 'json' && pathOpen && !loading && !error"
        :expression="expression"
        :match-count="matchCount"
        :suggestions="suggestions"
        @update:expression="expression = $event"
        @close="pathOpen = false"
      />

      <main class="viewer-area">
        <section v-if="loading" class="state">
          <v-circular-progress indeterminate />
          <span class="state-text">{{ $t('loading') }}</span>
        </section>

        <section v-else-if="error" class="state error">
          <i-material-symbols:error-outline-rounded class="state-icon" />
          <span class="state-text">{{ error }}</span>
          <v-outlined-button @click="retry">{{ $t('retry') }}</v-outlined-button>
        </section>

        <SplitPanels
          v-else-if="structured || kind === 'md'"
          :ratio="ratio"
          :folded="effectiveFolded"
          @update:ratio="ratio = $event"
        >
          <template #source>
            <ViewerPanel :title="$t('viewer_source')" :meta="sourceMeta">
              <template #actions>
                <v-copy-button class="mini-btn" :text="viewText" />
              </template>
              <CodeEditor :model-value="viewText" :language="cmLanguage" read-only :wrap-text="wrap" />
            </ViewerPanel>
          </template>
          <template #tree>
            <ViewerPanel v-if="kind === 'md'" :title="$t('preview')" meta="Markdown">
              <template #icon><i-lucide-eye /></template>
              <div class="md-container" v-html="renderedMarkdown"></div>
            </ViewerPanel>
            <ViewerPanel v-else :title="$t('viewer_tree')">
              <template #icon><i-lucide-list-tree /></template>
              <template #actions>
                <template v-if="!jsonInvalid">
                  <button class="mini-btn" :aria-label="$t('all')" @click="depth = 999">
                    <i-lucide-chevrons-down-up />
                  </button>
                  <button class="mini-btn" :aria-label="$t('depth')" @click="depth = 1">
                    <i-lucide-chevrons-up-down />
                  </button>
                </template>
              </template>
              <div v-if="jsonInvalid" class="panel-placeholder">
                <i-lucide-triangle-alert />
                <span>{{ $t('invalid_json_tree_hint') }}</span>
              </div>
              <json-viewer
                v-else
                :value="treeValue"
                :expand-depth="depth"
                :xml="kind === 'xml'"
                :xml-root="xmlRoot"
                :filter-paths="matchPaths"
              />
            </ViewerPanel>
          </template>
        </SplitPanels>

        <ViewerPanel v-else-if="kind === 'txt'" :title="$t('viewer_source')" :meta="$t('plain_text')" class="solo-panel">
          <template #icon><i-lucide-code /></template>
          <template #actions>
            <v-copy-button class="mini-btn" :text="viewText" />
          </template>
          <CodeEditor :model-value="viewText" read-only :wrap-text="wrap" />
        </ViewerPanel>

        <ViewerPanel v-else-if="kind === 'doc'" :title="$t('doc_preview_title')" class="solo-panel">
          <template #icon><i-lucide-file-text /></template>
          <div class="panel-placeholder tall">
            <i-lucide-file-text />
            <span class="placeholder-title">{{ $t('doc_preview_title') }}</span>
            <span class="placeholder-hint">{{ $t('doc_preview_hint') }}</span>
            <v-outlined-button class="placeholder-btn" @click="downloadFile">
              <i-lucide-download />
              {{ $t('download') }}
            </v-outlined-button>
          </div>
        </ViewerPanel>
      </main>

      <StatusBar
        v-if="!loading && !error"
        :kind="kind"
        :json-invalid="jsonInvalid"
        :error-pos="errorPos"
        :fixable="fixable"
        :lines="lines"
        :size-text="sizeText"
        @fix="fix"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { formatDateTime, formatFileSize, formatTimeAgo } from '@/lib/format'
import CodeEditor from '@/components/CodeEditor.vue'
import JsonViewer from '@/components/jsonviewer/json-viewer.vue'
import { useTextFile } from './text-file'
import { useViewer } from './useViewer'
import { kindBadge } from './file-kind'
import ViewerToolbar from './ViewerToolbar.vue'
import JsonPathBar from './JsonPathBar.vue'
import ViewerPanel from './ViewerPanel.vue'
import SplitPanels from './SplitPanels.vue'
import StatusBar from './StatusBar.vue'

const isTauri = __IS_TAURI__

const isFullscreen = ref(false)

const {
  loading, error, content, draft, fileName, lastModified,
  renderedMarkdown, saving,
  language,
  isEditing, dirty, displayTitle, statusText, canEdit, showInFinder,
  retry, openEditor, openViewer,
  downloadFile, revealInFinder, save, isLoggedIn,
} = useTextFile()

const viewer = useViewer({ fileName, content })
const {
  kind, structured, big, sizeBytes,
  jsonInvalid, errorPos, fixable, xmlTree,
  mode, depth, pathOpen, expression, wrap, indentSize, folded, ratio,
  viewText, suggestions, matchPaths, matchCount, canTransform, lines,
  cmLanguage,
  format, minify, fix,
} = viewer

const kindLabel = computed(() => kindBadge(kind.value))

const effectiveFolded = computed(() => {
  if (mode.value === 'source') return 'tree'
  if (mode.value === 'tree') return 'source'
  return folded.value
})

const treeValue = computed(() => (kind.value === 'xml' ? xmlTree.value?.value : viewer.jsonValue.value))
const xmlRoot = computed(() => (kind.value === 'xml' ? xmlTree.value?.rootTag ?? '' : ''))

const sourceMeta = computed(() => {
  switch (kind.value) {
    case 'json': return 'JSON'
    case 'xml': return 'XML'
    case 'md': return 'Markdown'
    default: return ''
  }
})

const sizeText = computed(() => formatFileSize(sizeBytes.value))
</script>

<style scoped>
.page {
  height: 100vh;
  height: 100dvh;
  display: flex;
  flex-direction: column;
  background: var(--md-sys-color-surface);
}

.topbar {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 56px;
  padding: 8px 16px;
  border-bottom: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface);
  flex-shrink: 0;
}

.page.fullscreen .topbar {
  display: none;
}

.topbar--tauri {
  padding-left: 80px;
  -webkit-app-region: drag;
}

.topbar--tauri .actions {
  -webkit-app-region: no-drag;
}

.file-chip {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
  flex: 1;
}

.file-icon {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  background: var(--md-sys-color-primary-container);
  color: var(--md-sys-color-on-primary-container);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  font-size: 20px;
}

.title-wrap {
  min-width: 0;
}

.title-line {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.title {
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--md-sys-color-on-surface);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.type-badge {
  font-size: 0.6875rem;
  font-weight: 700;
  letter-spacing: 0.4px;
  padding: 1px 8px;
  border-radius: 999px;
  background: var(--md-sys-color-primary-container);
  color: var(--md-sys-color-on-primary-container);
  flex-shrink: 0;

  &.plain {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface-variant);
  }
}

.meta {
  display: flex;
  gap: 8px;
  margin-top: 2px;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.75rem;
  min-height: 1em;
}

.actions {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.action-btn {
  white-space: nowrap;
}

.status {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.85rem;
  white-space: nowrap;
}

.status-icon {
  font-size: 18px;
  color: var(--md-sys-color-primary);
}

.status.saving .status-icon {
  display: none;
}

.status-text {
  line-height: 1;
}

.editor {
  flex: 1;
  min-width: 0;
  min-height: 0;
}

.viewer-area {
  flex: 1;
  min-height: 0;
  display: flex;
  padding: 8px;
  gap: 8px;
  overflow: hidden;
}

.solo-panel {
  flex: 1;
  min-width: 0;
}

.state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  text-align: center;
}

.state-text {
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.875rem;
}

.state.error .state-icon {
  font-size: 48px;
  color: var(--md-sys-color-error);
}

.state.error .state-text {
  color: var(--md-sys-color-error);
  max-width: 460px;
}

.mini-btn {
  border: none;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  width: 24px;
  height: 24px;
  border-radius: 6px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-size: 14px;
  padding: 0;

  &:hover {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }
}

.panel-placeholder {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--md-sys-color-outline);
  font-size: 0.8125rem;
  padding: 16px;
  text-align: center;

  > svg {
    width: 40px;
    height: 40px;
    opacity: 0.6;
  }

  .placeholder-title {
    font-weight: 600;
    color: var(--md-sys-color-on-surface-variant);
  }

  .placeholder-hint {
    font-size: 0.75rem;
  }

  .placeholder-btn {
    margin-top: 8px;
  }

  &.tall {
    height: 100%;
  }
}

.viewer-area :deep(.CodeMirror),
.viewer-area :deep(.cm-editor) {
  height: 100%;
}

@media (max-width: 768px) {
  .topbar {
    padding: 8px 12px;
  }

  .meta {
    display: none;
  }

  .viewer-area {
    padding: 4px;
  }
}
</style>
