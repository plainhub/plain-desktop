<!-- eslint-disable vue/no-v-html -->
<template>
  <div class="page">
    <header
      class="topbar"
      :class="{ 'topbar--tauri': isTauri }"
      :data-tauri-drag-region="isTauri ? 'deep' : null"
    >
      <div class="title-wrap">
        <div class="title">{{ displayTitle }}</div>
        <div class="meta">
          <span v-if="fileSize">{{ formatFileSize(fileSize) }}</span>
          <span v-if="lastModified" v-tooltip="formatDateTime(lastModified)">{{ formatTimeAgo(lastModified) }}</span>
        </div>
      </div>

      <div v-if="statusText" class="status" :class="{ saving: saving }">
        <v-circular-progress v-if="saving" indeterminate :size="16" :width="2" />
        <i-material-symbols:check-circle-rounded v-else class="status-icon" />
        <span class="status-text">{{ statusText }}</span>
      </div>

      <div class="actions">
        <template v-if="!isEditing">
          <v-outlined-button v-if="showInFinder" class="action-btn" @click="revealInFinder">
            <i-lucide-folder-open />
            {{ $t('show_in_finder') }}
          </v-outlined-button>
          <v-outlined-button v-else class="action-btn" @click="downloadFile">
            <i-lucide-download />
            {{ $t('download') }}
          </v-outlined-button>
        </template>

        <template v-if="isEditing">
          <v-outlined-button class="action-btn" :loading="saving" :disabled="saving || !dirty" @click="save">
            {{ $t('save') }}
          </v-outlined-button>
          <v-outlined-button class="action-btn" @click="openViewer">
            {{ $t('view') }}
          </v-outlined-button>
        </template>
        <template v-else>
          <v-outlined-button v-if="canToggleView" class="action-btn" @click="toggleViewMode">
            <i-lucide-eye v-if="showRawText" />
            <i-lucide-code v-else />
            {{ showRawText ? $t('formatted_view') : $t('raw_text') }}
          </v-outlined-button>

          <v-outlined-button
            v-if="showRawText || (!isJsonFile && !isMarkdownFile)"
            class="action-btn"
            @click="toggleTextWrap"
          >
            <i-lucide-wrap-text />
            {{ textWrap ? $t('unwrap') : $t('wrap') }}
          </v-outlined-button>

          <v-outlined-button v-if="canEdit" class="action-btn" @click="openEditor">
            <i-lucide-pencil />
            {{ $t('edit') }}
          </v-outlined-button>
        </template>

        <header-actions :logged-in="isLoggedIn" />
      </div>
    </header>

    <main class="main">
      <section v-if="loading" class="state">
        <v-circular-progress indeterminate />
        <span class="state-text">{{ $t('loading') }}</span>
      </section>

      <section v-else-if="error" class="state error">
        <i-material-symbols:error-outline-rounded class="state-icon" />
        <span class="state-text">{{ error }}</span>
        <v-outlined-button @click="retry">{{ $t('retry') }}</v-outlined-button>
      </section>

      <section v-else-if="isEditing" class="editor">
        <CodeEditor v-model="draft" :language="language" />
      </section>

      <section v-else class="viewer">
        <div class="viewer-card">
          <pre
            v-if="showRawText || (!isJsonFile && !isMarkdownFile) || (isJsonFile && !jsonData)"
            class="view-raw text-view"
            :class="{ 'text-wrap': textWrap }"
          >{{ content }}</pre>
          <json-viewer v-else-if="isJsonFile" :value="jsonData" :expand-depth="2" />
          <div v-else-if="isMarkdownFile" class="md-container" v-html="renderedMarkdown"></div>
        </div>
      </section>
    </main>
  </div>
</template>

<script setup lang="ts">
import { formatDateTime, formatFileSize, formatTimeAgo } from '@/lib/format'
import JsonViewer from '@/components/jsonviewer/json-viewer.vue'
import { useTextFile } from './text-file'

const isTauri = __IS_TAURI__

const {
  loading, error, content, draft, fileName, fileSize, lastModified,
  jsonData, renderedMarkdown, showRawText, textWrap, saving,
  isJsonFile, isMarkdownFile, canToggleView, language,
  isEditing, dirty, displayTitle, statusText, canEdit, showSavedPulse, showInFinder,
  retry, openEditor, openViewer, toggleViewMode, toggleTextWrap,
  downloadFile, revealInFinder, save, isLoggedIn,
} = useTextFile()
</script>

<style scoped>
.page {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--md-sys-color-surface);
}

.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface);
}

.topbar--tauri {
  padding-left: 80px;
  -webkit-app-region: drag;
}

.topbar--tauri .actions {
  -webkit-app-region: no-drag;
}

.title-wrap {
  flex: 1;
  min-width: 0;
}

.title {
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--md-sys-color-on-surface);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.meta {
  display: flex;
  gap: 8px;
  margin-top: 2px;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 0.8rem;
  flex-wrap: wrap;
  min-height: 1em;
}

.meta span:not(:last-child)::after {
  content: '•';
  margin-left: 8px;
  color: var(--md-sys-color-outline);
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

.main {
  flex: 1;
  min-height: 0;
  display: flex;
}

.editor {
  flex: 1;
  min-width: 0;
  min-height: 0;
}

.viewer {
  flex: 1;
  min-width: 0;
  overflow: auto;
  padding: 20px;
  background: var(--md-sys-color-surface);
}

.viewer-card {
  max-width: 1200px;
  margin: 0 auto;
  background: var(--md-sys-color-surface-container-lowest);
  border-radius: 12px;
  padding: 24px;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.text-view {
  color: var(--md-sys-color-on-surface);
  white-space: pre;
  word-wrap: normal;
  margin: 0;
  overflow-x: auto;
}

.text-view.text-wrap {
  white-space: pre-wrap;
  word-wrap: break-word;
}

.state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
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

@media (max-width: 768px) {
  .topbar {
    padding: 10px 12px;
  }
  .viewer {
    padding: 16px;
  }
  .viewer-card {
    padding: 16px;
  }
}
</style>