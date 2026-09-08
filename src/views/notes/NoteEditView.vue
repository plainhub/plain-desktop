<!-- eslint-disable vue/no-v-html -->
<template>
  <div class="note-edit-page">
    <div class="top-app-bar">
      <v-icon-button v-tooltip="$t('back')" @click.prevent="backToList">
        <i-material-symbols:arrow-back-rounded />
      </v-icon-button>
      <div class="title">
        <input v-model="title" class="title-input" :placeholder="$t('title')" maxlength="250" style="display: none;" />
        <span v-show="notSaved" class="state-point">*</span>
        <field-id v-if="note?.updatedAt" :id="getTime()" class="time" :raw="note" />
      </div>
      <div class="actions">
        <item-tags :tags="note?.tags" :type="dataType" :only-links="true" />
        <note-mode-toggle :mode="viewMode" @update:mode="setViewMode" />
        <template v-if="id">
          <TagRelationsDropdown :type="dataType" :tags="tags ?? []" :item="{ key: note?.id ?? '', title: '', size: 0 }" :selected="note?.tags ?? []" />
          <v-icon-button v-tooltip="$t('print')" @click.prevent="print">
            <i-material-symbols:print-outline-rounded />
          </v-icon-button>
        </template>
      </div>
    </div>
    <div class="note-edit-container" :class="viewMode">
      <div v-show="viewMode !== 'preview'" class="editor-pane">
        <markdown-editor
          ref="editorRef"
          v-model="content"
          :placeholder="$t('write_markdown')"
          @paste-images="onPasteImages"
        />
        <div v-if="uploadingImage" class="upload-indicator">
          <v-circular-progress indeterminate class="sm" /> {{ $t('uploading') }}
        </div>
      </div>
      <div v-show="viewMode !== 'editor'" class="preview-pane md-container" v-html="markdown" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useNoteEdit } from '@/hooks/note-edit'
import type MarkdownEditorVue from '@/components/MarkdownEditor.vue'
import NoteModeToggle from '@/views/notes/NoteModeToggle.vue'

const editorRef = ref<InstanceType<typeof MarkdownEditorVue>>()

const {
  id, note, title, content, markdown, notSaved, dataType, tags, viewMode, uploadingImage,
  backToList, getTime, print, handlePasteImages, setViewMode,
} = useNoteEdit()

async function onPasteImages(files: File[]) {
  const paths = await handlePasteImages(files)
  for (const md of paths) {
    editorRef.value?.insertText('\n' + md + '\n')
  }
}
</script>

<style lang="scss">
.page-content .main-note-entry {
  flex-direction: row;

  .note-edit-page {
    flex: 1;
    width: 0;
    min-width: 0;
  }
}
</style>
<style lang="scss" scoped>
.title-input {
  flex: 1;
  border: none;
  outline: none;
  font-size: 1rem;
  font-weight: 500;
  background: transparent;
  color: var(--md-sys-color-on-surface);
  padding: 4px 8px;
  border-radius: 4px;
  min-width: 200px;
  &:focus {
    background: var(--md-sys-color-surface-variant);
  }
  &::placeholder {
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.6;
  }
}
.time {
  margin-left: 8px;
  font-size: 0.875rem;
  font-weight: normal;
}

.state-point {
  color: red;
}
.note-edit-page {
  display: flex;
  flex-direction: column;
  gap: 12px;
  height: calc(100dvh - 64px);
  padding: 12px 16px 16px;
  box-sizing: border-box;
  overflow: hidden;
}
.note-edit-container {
  flex: 1;
  min-height: 0;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: 14px;
  overflow: hidden;
  background: var(--md-sys-color-surface);
  &.preview {
    border: none;
  }
  &.editor .editor-pane {
    display: block;
  }
  &.preview .preview-pane {
    display: block;
  }
}
.editor-pane {
  height: 100%;
  position: relative;
  overflow: hidden;
}
.preview-pane {
  height: 100%;
  padding: 20px 24px;
  box-sizing: border-box;
  overflow-y: auto;
  position: relative;
}
.upload-indicator {
  position: absolute;
  bottom: 8px;
  left: 12px;
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 0.8rem;
  color: var(--md-sys-color-on-surface-variant);
  background: var(--md-sys-color-surface-variant);
  padding: 4px 10px;
  border-radius: 6px;
}
@media (max-width: 960px) {
  .note-edit-page {
    padding: 8px 10px 10px;
  }
  .preview-pane {
    padding: 14px;
  }
}
</style>
