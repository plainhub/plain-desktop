<template>
  <div class="actions">
    <template v-if="item.isDir">
      <v-icon-button v-tooltip="$t('download')" class="sm" @click.stop="downloadDir(item.path)">
        <i-material-symbols:download-rounded />
      </v-icon-button>
      <v-dropdown v-if="!inZip" v-model="uploadMenuVisible">
        <template #trigger>
          <v-icon-button v-tooltip="$t('upload')" class="sm">
            <i-material-symbols:upload-rounded />
          </v-icon-button>
        </template>
        <div class="dropdown-item" @click.stop="uploadFiles(item.path); uploadMenuVisible = false">
          {{ $t('upload_files') }}
        </div>
        <div class="dropdown-item" @click.stop="uploadDir(item.path); uploadMenuVisible = false">
          {{ $t('upload_folder') }}
        </div>
      </v-dropdown>
    </template>
    <template v-else>
      <v-icon-button v-tooltip="$t('download')" class="sm" @click.stop="downloadFile(item.path)">
        <i-material-symbols:download-rounded />
      </v-icon-button>
    </template>

    <v-icon-button v-if="!inZip" :id="deleteAnchorId" v-tooltip="$t('delete')" class="sm" @click.stop="confirming = true">
      <i-material-symbols:delete-forever-outline-rounded />
    </v-icon-button>
    <v-dropdown-menu v-if="!inZip" v-model="confirming" :anchor="deleteAnchorId">
      <inline-delete-confirm :name="item.name" @confirm="onConfirmDelete" @cancel="confirming = false" />
    </v-dropdown-menu>

    <v-dropdown v-model="infoMenuVisible">
      <template #trigger>
        <v-icon-button v-tooltip="$t('info')" class="sm">
          <i-material-symbols:info-outline-rounded />
        </v-icon-button>
      </template>
      <section class="card card-info">
        <div class="key-value vertical">
          <div class="key">{{ $t('path') }}</div>
          <div class="value">
            {{ item.path }}
          </div>
        </div>
      </section>
    </v-dropdown>

    <v-dropdown v-if="!inZip" v-model="actionsMenuVisible">
      <template #trigger>
        <v-icon-button v-tooltip="$t('actions')" class="sm">
          <i-material-symbols:more-vert />
        </v-icon-button>
      </template>
      <div class="dropdown-item" @click.stop="duplicateItem(item); actionsMenuVisible = false">
        {{ $t('duplicate') }}
      </div>
      <div class="dropdown-item" @click.stop="cutItem(item); actionsMenuVisible = false">
        {{ $t('cut') }}
      </div>
      <div class="dropdown-item" @click.stop="copyItem(item); actionsMenuVisible = false">
        {{ $t('copy') }}
      </div>
      <div v-if="item.isDir && canPaste" class="dropdown-item" @click.stop="pasteItem(item); actionsMenuVisible = false">
        {{ $t('paste') }}
      </div>
      <div v-if="!item.isDir && isTauri" class="dropdown-item" @click.stop="saveAs(item); actionsMenuVisible = false">
        {{ $t('save_as') }}
      </div>
      <div v-if="!item.isDir" class="dropdown-item" @click.stop="copyLink(item); actionsMenuVisible = false">
        {{ $t('copy_link') }}
      </div>
      <div v-if="item.isDir" class="dropdown-item" @click.stop="addToFavorites(item); actionsMenuVisible = false">
        {{ $t('add_to_favorites') }}
      </div>
      <div class="dropdown-item" @click.stop="renameItem(item); actionsMenuVisible = false">
        {{ $t('rename') }}
      </div>
    </v-dropdown>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { IFile } from '@/lib/file'

interface Props {
  item: IFile
  canPaste: boolean
  inZip?: boolean
}

const props = defineProps<Props>()

const isTauri = __IS_TAURI__

const emit = defineEmits<{
  downloadDir: [path: string]
  downloadFile: [path: string]
  saveAs: [item: IFile]
  uploadFiles: [path: string]
  uploadDir: [path: string]
  deleteItem: [item: IFile]
  duplicateItem: [item: IFile]
  cutItem: [item: IFile]
  copyItem: [item: IFile]
  pasteItem: [item: IFile]
  copyLink: [item: IFile]
  renameItem: [item: IFile]
  addToFavorites: [item: IFile]
}>()

const uploadMenuVisible = ref(false)
const infoMenuVisible = ref(false)
const actionsMenuVisible = ref(false)
const confirming = ref(false)
const deleteAnchorId = computed(() => `file-delete-${props.item.id}`)

function downloadDir(path: string) {
  emit('downloadDir', path)
}

function downloadFile(path: string) {
  emit('downloadFile', path)
}

function saveAs(item: IFile) {
  emit('saveAs', item)
}

function uploadFiles(path: string) {
  emit('uploadFiles', path)
}

function uploadDir(path: string) {
  emit('uploadDir', path)
}

function onConfirmDelete() {
  emit('deleteItem', props.item)
  confirming.value = false
}

function duplicateItem(item: IFile) {
  emit('duplicateItem', item)
}

function cutItem(item: IFile) {
  emit('cutItem', item)
}

function copyItem(item: IFile) {
  emit('copyItem', item)
}

function pasteItem(item: IFile) {
  emit('pasteItem', item)
}

function copyLink(item: IFile) {
  emit('copyLink', item)
}

function renameItem(item: IFile) {
  emit('renameItem', item)
}

function addToFavorites(item: IFile) {
  emit('addToFavorites', item)
}
</script>

<style scoped lang="scss">
.actions {
  display: flex;
  gap: 4px;
  align-items: center;

  &.mobile {
    flex-wrap: wrap;
  }
}
</style>
