<template>
  <v-modal @close="close">
    <template #headline>{{ $t('exclude_directories') }}</template>
    <template #content>
      <div class="excluded-dirs">
        <p class="hint">{{ $t('excluded_dirs_hint') }}</p>
        <p v-if="store.excludedDirs.length === 0" class="empty-hint">{{ $t('no_excluded_dirs') }}</p>
        <div v-for="dir in store.excludedDirs" :key="dir" class="dir-row">
          <i-lucide:folder-minus class="dir-icon" />
          <span class="dir-path">{{ dir }}</span>
          <button class="remove-btn" :aria-label="$t('remove')" @click="removeDir(dir)">
            <i-lucide:x />
          </button>
        </div>
        <button class="add-btn" @click="addDir">
          <i-lucide:plus />
          <span>{{ $t('add_excluded_dir') }}</span>
        </button>
      </div>
    </template>
    <template #actions>
      <v-outlined-button @click="close">{{ $t('close') }}</v-outlined-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { popModal, promptModal } from '@/components/modal'
import { useMainStore } from '@/stores/main'
import * as prefs from '@/lib/prefs/core'
import toast from '@/components/toaster'
import DirectoryPickerModal from '@/components/DirectoryPickerModal.vue'

const store = useMainStore()

async function save(paths: string[]) {
  try {
    await prefs.set('file_index_excluded_dirs', paths)
    store.excludedDirs = paths
  } catch (error) {
    toast(error instanceof Error ? error.message : String(error), 'error')
  }
}

function removeDir(dir: string) {
  return save(store.excludedDirs.filter((d) => d !== dir))
}

async function addDir() {
  const selected = await promptModal<string>(DirectoryPickerModal, {
    title: '',
    description: '',
    initialPath: '',
    modalId: 'excluded-dir-picker',
  })
  if (typeof selected === 'string' && selected.trim()) {
    const normalized = selected.trim().endsWith('/') ? selected.trim() : selected.trim() + '/'
    if (!store.excludedDirs.includes(normalized)) {
      await save([...store.excludedDirs, normalized])
    }
  }
}

function close() {
  popModal()
}
</script>

<style lang="scss" scoped>
.excluded-dirs {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 320px;
}

.hint {
  font-size: 0.875rem;
  color: var(--md-sys-color-outline);
  margin: 0 0 8px;
}

.empty-hint {
  font-size: 0.875rem;
  color: var(--md-sys-color-outline);
  margin: 0 0 8px;
}

.dir-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  border-radius: 8px;
  background: var(--md-sys-color-surface-container);

  &:hover { background: var(--md-sys-color-surface-container-high); }
}

.dir-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  color: var(--md-sys-color-on-surface-variant);
}

.dir-path {
  flex: 1;
  font-size: 0.875rem;
  font-family: monospace;
  color: var(--md-sys-color-on-surface);
  word-break: break-all;
}

.remove-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  border: none;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  cursor: pointer;
  flex-shrink: 0;

  &:hover {
    background: color-mix(in srgb, var(--md-sys-color-error) 12%, transparent);
    color: var(--md-sys-color-error);
  }
}

.add-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  border-radius: 20px;
  border: none;
  background: color-mix(in srgb, var(--md-sys-color-primary) 10%, transparent);
  color: var(--md-sys-color-primary);
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  margin-top: 4px;
  transition: background 0.15s;

  &:hover { background: color-mix(in srgb, var(--md-sys-color-primary) 18%, transparent); }

  svg { width: 16px; height: 16px; }
}
</style>
