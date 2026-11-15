<template>
  <template v-if="placement === 'top'">
    <v-dropdown v-if="upload && !state.filterTrash && (!isPhone || !state.checked)" v-model="uploadMenu">
      <template #trigger>
        <v-icon-button v-tooltip="$t('upload')">
          <i-material-symbols:upload-rounded />
        </v-icon-button>
      </template>
      <div class="upload-menu-content">
        <div class="dropdown-item" @click.stop="handleUploadFiles">
          {{ $t('upload_files') }}
        </div>
        <div class="dropdown-item" @click.stop="handleUploadDir">
          {{ $t('upload_folder') }}
        </div>
        <div v-if="upload.onEditDir || upload.dir" class="upload-dest-row">
          <span class="upload-dest-label">{{ $t('save_to') }}</span>
          <span class="upload-dest-path" :title="upload.dir || ''">{{ upload.dir || '—' }}</span>
          <v-icon-button v-if="upload.editable && upload.onEditDir" v-tooltip="$t('edit')" class="upload-dest-edit" @click.stop="handleEditUploadDir">
            <i-material-symbols:edit-outline-rounded />
          </v-icon-button>
        </div>
      </div>
    </v-dropdown>

    <v-dropdown v-if="!state.checked && !hideMoreMenu && !options?.show" v-model="moreMenu">
      <template #trigger>
        <v-icon-button v-tooltip="$t('sort')">
          <i-material-symbols:sort-rounded />
        </v-icon-button>
      </template>

      <div class="dropdown-item" @click.stop="handleOpenKeyboardShortcuts">
        {{ $t('keyboard_shortcuts') }}
      </div>

      <div
        v-for="item in sort.items"
        :key="item.value"
        class="dropdown-item"
        :class="{ 'selected': item.value === sort.sortBy }"
        @click.stop="handleSort(item.value)"
      >
        {{ $t(item.label) }}
      </div>
    </v-dropdown>



    <ViewToggleButtons
      v-if="view?.show && !isPhone && !view.hide"
      :card-view="safeCardView"
      @update:card-view="(value: boolean) => view?.onUpdateCardView(value)"
    />

    <ViewOptionsPanel
      v-if="!state.checked && options?.show"
      :show-group-by="options.showGroupBy"
      :group-by-items="options.groupByItems"
      :group-by="options.groupBy"
      :scroll-paging="options.scrollPaging"
      :sort-by="sort.sortBy"
      :sort-items="sort.items"
      :on-open-keyboard-shortcuts="openKeyboardShortcuts"
      @update:group-by="options.onUpdateGroupBy?.($event)"
      @update:scroll-paging="options.onUpdateScrollPaging($event)"
      @update:sort-by="sort.onSort($event)"
    />
  </template>

  <template v-else>
    <template v-if="state.filterTrash">
      <v-dropdown v-model="moreMenu">
        <template #trigger>
          <v-icon-button v-tooltip="$t('sort')">
            <i-material-symbols:sort-rounded />
          </v-icon-button>
        </template>

        <div class="dropdown-item" @click.stop="handleOpenKeyboardShortcuts">
          {{ $t('keyboard_shortcuts') }}
        </div>

        <div
          v-for="item in sort.items"
          :key="item.value"
          class="dropdown-item"
          :class="{ 'selected': item.value === sort.sortBy }"
          @click.stop="handleSort(item.value)"
        >
          {{ $t(item.label) }}
        </div>
      </v-dropdown>
    </template>

    <ViewToggleButtons
      v-else-if="view?.show && !view.hide"
      :card-view="safeCardView"
      @update:card-view="(value: boolean) => view?.onUpdateCardView(value)"
    />
  </template>
</template>

<script setup lang="ts">
import { computed, inject, ref, type Ref } from 'vue'
import ViewToggleButtons from '@/components/ViewToggleButtons.vue'
import ViewOptionsPanel from '@/components/media/ViewOptionsPanel.vue'
import { openModal } from '@/components/modal'
import KeyboardShortcutsModal from '@/components/KeyboardShortcutsModal.vue'
import { mediaKeyboardShortcuts } from '@/lib/shortcuts/media'
import { useI18n } from 'vue-i18n'

type SortItem = { label: string; value: string }
type GroupByItem = { label: string; value: string }
type Placement = 'top' | 'secondary'
type ActionsState = { filterTrash: boolean; checked: boolean }
type SortConfig = { sortBy: string; items: SortItem[]; onSort: (value: string) => void }
type UploadConfig = { dir?: string; editable?: boolean; onUploadFiles: () => void; onUploadDir: () => void; onEditDir?: () => void }
type ViewConfig = { show?: boolean; cardView?: boolean; hide?: boolean; onUpdateCardView: (value: boolean) => void }
type OptionsConfig = {
  show?: boolean
  showGroupBy?: boolean
  groupByItems?: GroupByItem[]
  groupBy?: string
  scrollPaging: boolean
  onUpdateGroupBy?: (value: string) => void
  onUpdateScrollPaging: (value: boolean) => void
}

const props = defineProps<{
  placement: Placement
  state: ActionsState
  sort: SortConfig
  upload?: UploadConfig
  view?: ViewConfig
  options?: OptionsConfig
  hideMoreMenu?: boolean
}>()

const isPhone = inject<Ref<boolean>>('isPhone')!
const { t } = useI18n()
const uploadMenu = ref(false)
const moreMenu = ref(false)
const safeCardView = computed(() => props.view?.cardView ?? false)

function openKeyboardShortcuts() {
  openModal(KeyboardShortcutsModal, { title: t('keyboard_shortcuts'), shortcuts: mediaKeyboardShortcuts })
}
function handleUploadFiles() {
  props.upload?.onUploadFiles()
  uploadMenu.value = false
}
function handleUploadDir() {
  props.upload?.onUploadDir()
  uploadMenu.value = false
}
function handleEditUploadDir() {
  props.upload?.onEditDir?.()
  uploadMenu.value = false
}
function handleOpenKeyboardShortcuts() {
  openKeyboardShortcuts()
  moreMenu.value = false
}
function handleSort(value: string) {
  props.sort.onSort(value)
  moreMenu.value = false
}
</script>

<style scoped>
.upload-menu-content {
  max-width: min(320px, calc(100vw - 40px));
}

.upload-dest-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  border-top: 1px solid var(--md-sys-color-outline-variant);
  min-height: 56px;
}

.upload-dest-label {
  font-size: 0.8rem;
  color: var(--md-sys-color-on-surface-variant);
  white-space: nowrap;
  flex-shrink: 0;
}

.upload-dest-path {
  font-size: 0.8rem;
  color: var(--md-sys-color-on-surface);
  flex: 1;
  min-width: 0;
  white-space: normal;
  overflow-wrap: anywhere;
  font-weight: 600;
}
</style>
