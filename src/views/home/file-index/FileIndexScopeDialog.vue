<template>
  <v-modal @close="cancel">
    <template #headline>{{ $t('file_index.scope') }} · {{ volumeName }}</template>
    <template #content>
      <div class="scope-content">
        <div class="modes" role="group" :aria-label="$t('file_index.mode')">
          <button v-for="mode in modes" :key="mode.value" :aria-pressed="draft.indexMode === mode.value" :disabled="saving" @click="draft.indexMode = mode.value">{{ $t(mode.label) }}</button>
        </div>
        <p class="subtle">{{ $t(`file_index.${draft.indexMode === 'ALLOWLIST' ? 'allow_hint' : draft.indexMode === 'BLOCKLIST' ? 'block_hint' : 'disabled_hint'}`) }}</p>
        <template v-if="draft.indexMode !== 'DISABLED'">
          <div v-for="path in activeDirs" :key="path" class="directory-row">
            <span>{{ path }}</span>
            <v-icon-button :disabled="saving" :aria-label="$t('remove')" @click="remove(path)"><i-material-symbols:close-rounded /></v-icon-button>
          </div>
          <v-text-button :disabled="saving || !draft.isAvailable" @click="add">{{ $t('file_index.add_directory') }}</v-text-button>
        </template>
        <FileIndexDefaults :rules="defaults" />
        <p v-if="error" role="alert" class="error">{{ error }}</p>
      </div>
    </template>
    <template #actions>
      <v-text-button :disabled="saving" @click="cancel">{{ $t('cancel') }}</v-text-button>
      <v-filled-button :loading="saving" @click="save">{{ $t(volume.indexMode === 'UNCONFIGURED' && canScanVolume(draft) ? 'file_index.create' : 'save') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { closeById, modalQueue, promptModal } from '@/components/modal'
import FileIndexDefaults from './FileIndexDefaults.vue'
import type { FileIndexDefaults as DefaultRules } from '@/lib/api/file-index'
import DirectoryPickerModal from '@/components/DirectoryPickerModal.vue'
import { bindGraphqlClient } from '@/lib/api/gql-client'
import { canScanVolume, saveFileIndexVolume, type FileIndexVolume, type IndexMode } from '@/lib/api/file-index'

const props = defineProps<{ modalId: string; volume: FileIndexVolume; defaults: DefaultRules; volumeName: string; onSaved: () => unknown }>()
const modalId = Number(props.modalId.replace(/\D/g, ''))
const request = bindGraphqlClient()
const draft = ref<FileIndexVolume>(copy(props.volume))
const saving = ref(false)
const error = ref('')
const modes: { value: IndexMode; label: string }[] = [
  { value: 'ALLOWLIST', label: 'file_index.allowlist' },
  { value: 'BLOCKLIST', label: 'file_index.blocklist' },
  { value: 'DISABLED', label: 'file_index.disabled' },
]
const activeDirs = computed(() => (draft.value.indexMode === 'ALLOWLIST' ? draft.value.includedDirs : draft.value.excludedDirs))

function copy(volume: FileIndexVolume): FileIndexVolume {
  return { ...volume, indexMode: volume.indexMode === 'UNCONFIGURED' ? 'BLOCKLIST' : volume.indexMode, includedDirs: [...volume.includedDirs], excludedDirs: [...volume.excludedDirs] }
}

function remove(path: string) {
  if (draft.value.indexMode === 'ALLOWLIST') draft.value.includedDirs = activeDirs.value.filter((item) => item !== path)
  else draft.value.excludedDirs = activeDirs.value.filter((item) => item !== path)
}

async function add() {
  const path = await promptModal<string>(DirectoryPickerModal, {
    initialPath: draft.value.path,
    volume: { mountPoint: props.volume.path, name: props.volumeName },
  })
  if (typeof path === 'string' && !activeDirs.value.includes(path)) activeDirs.value.push(path)
}

function cancel() {
  if (!saving.value) void closeById(modalId)
}

async function save() {
  saving.value = true
  error.value = ''
  try {
    await saveFileIndexVolume(draft.value, request)
    await props.onSaved()
    if (modalQueue.value.some((modal) => modal.id === modalId)) await closeById(modalId)
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause)
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.scope-content {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.modes {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 8px;
}
.modes button {
  padding: 8px 16px;
  border: 0;
  border-radius: 999px;
  background: transparent;
  color: var(--md-sys-color-on-surface);
  font: inherit;
  cursor: pointer;
}
.modes button[aria-pressed='true'] {
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}
.directory-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.directory-row span {
  flex: 1;
  min-width: 0;
  overflow-wrap: anywhere;
}
.error {
  color: var(--md-sys-color-error);
  margin: 0;
}
</style>
