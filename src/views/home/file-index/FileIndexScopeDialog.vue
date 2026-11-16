<template>
  <v-modal @close="cancel">
    <template #headline>{{ $t('file_index.scan_scope_question') }}</template>
    <template #content>
      <div class="scope-content">
        <p class="volume-name">{{ volumeName }}</p>
        <div role="radiogroup" :aria-label="$t('file_index.scan_scope_question')">
          <div v-for="choice in choices" :key="choice.mode">
            <label class="scope-choice">
              <input
                type="radio"
                :name="radioName"
                :value="choice.mode"
                :checked="mode === choice.mode"
                :disabled="!!saving"
                @change="mode = choice.mode"
              />
              {{ $t(choice.label) }}
            </label>
            <div v-if="mode === choice.mode" class="scope-directories">
              <div v-for="path in draft[choice.field]" :key="path" class="directory-row">
                <span>{{ path }}</span>
                <v-icon-button
                  :disabled="!!saving"
                  :aria-label="`${$t('remove')}: ${path}`"
                  @click="draft[choice.field] = draft[choice.field].filter((item) => item !== path)"
                ><i-material-symbols:close-rounded /></v-icon-button>
              </div>
              <v-text-button :disabled="!!saving || !draft.isAvailable" @click="add(choice.field)">
                <i-material-symbols:add-rounded aria-hidden="true" />{{ $t(choice.action) }}
              </v-text-button>
              <small v-if="draft[choice.field].length">{{ $t('file_index.includes_subfolders') }}</small>
            </div>
          </div>
        </div>
        <FileIndexDefaults :rules="defaults" />
        <div v-if="canDisable">
          <v-text-button :loading="saving === 'disable'" :disabled="!!saving || !volume.isAvailable" @click="disable">
            {{ $t('file_index.disable') }}
          </v-text-button>
        </div>
        <p v-if="error" role="alert" class="error">{{ error }}</p>
      </div>
    </template>
    <template #actions>
      <v-text-button :disabled="!!saving" @click="cancel">{{ $t('cancel') }}</v-text-button>
      <v-filled-button :loading="saving === 'scope'" :disabled="!!saving || !canSave" @click="save">{{ $t('save') }}</v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed, ref, useId } from 'vue'
import { closeById, modalQueue, promptModal } from '@/components/modal'
import FileIndexDefaults from './FileIndexDefaults.vue'
import type { FileIndexDefaults as DefaultRules } from '@/lib/api/file-index'
import DirectoryPickerModal from '@/components/DirectoryPickerModal.vue'
import { bindGraphqlClient } from '@/lib/api/gql-client'
import { saveFileIndexVolume, type FileIndexVolume } from '@/lib/api/file-index'

const props = defineProps<{ modalId: string; volume: FileIndexVolume; defaults: DefaultRules; volumeName: string; onSaved: () => unknown }>()
const modalId = Number(props.modalId.replace(/\D/g, ''))
const request = bindGraphqlClient()
const radioName = useId()
const draft = ref({ ...props.volume, includedDirs: [...props.volume.includedDirs], excludedDirs: [...props.volume.excludedDirs] })
const mode = ref<'ALLOWLIST' | 'BLOCKLIST' | null>(
  draft.value.indexMode === 'DISABLED' ? null : draft.value.indexMode === 'ALLOWLIST' ? 'ALLOWLIST' : 'BLOCKLIST',
)
const saving = ref<'scope' | 'disable' | null>(null)
const error = ref('')
const choices = [
  { mode: 'BLOCKLIST', label: 'file_index.blocklist', field: 'excludedDirs', action: 'file_index.exclude_folder' },
  { mode: 'ALLOWLIST', label: 'file_index.allowlist', field: 'includedDirs', action: 'file_index.select_folder' },
] as const
const canDisable = computed(() => props.volume.indexMode === 'ALLOWLIST' || props.volume.indexMode === 'BLOCKLIST')
const canSave = computed(() => mode.value === 'BLOCKLIST' || (mode.value === 'ALLOWLIST' && draft.value.includedDirs.length > 0))

async function add(field: 'includedDirs' | 'excludedDirs') {
  const path = await promptModal<string>(DirectoryPickerModal, {
    initialPath: draft.value.path,
    volume: { mountPoint: props.volume.path, name: props.volumeName },
  })
  if (typeof path === 'string' && !draft.value[field].includes(path)) draft.value[field].push(path)
}

function cancel() {
  if (!saving.value) void closeById(modalId)
}

async function save() {
  if (!mode.value || !canSave.value) return
  await apply({ ...draft.value, indexMode: mode.value }, 'scope')
}

async function disable() {
  if (!canDisable.value || !props.volume.isAvailable) return
  await apply({ ...props.volume, indexMode: 'DISABLED' }, 'disable')
}

async function apply(volume: FileIndexVolume, action: 'scope' | 'disable') {
  if (saving.value) return
  saving.value = action
  error.value = ''
  try {
    await saveFileIndexVolume(volume, request)
    await props.onSaved()
    if (modalQueue.value.some((modal) => modal.id === modalId)) await closeById(modalId)
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause)
  } finally {
    saving.value = null
  }
}
</script>

<style scoped>
.scope-content {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.volume-name {
  margin: 0;
}
.volume-name,
.scope-directories small {
  color: var(--md-sys-color-on-surface-variant);
}
.scope-choice {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 0;
  cursor: pointer;
}
.scope-choice input {
  margin: 0;
  flex: none;
  accent-color: var(--md-sys-color-primary);
}
.scope-directories {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
  padding-inline-start: 24px;
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
