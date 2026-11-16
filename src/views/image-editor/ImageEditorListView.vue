<template>
  <div class="image-editor-list">
    <Teleport v-if="isActive" to="#header-start-slot" defer>
      <div class="header-title">{{ $t('image_editor.title') }}</div>
    </Teleport>

    <div class="list-content">
      <ImageDropZone
        accept=".jpg,.jpeg,.png,.webp,.bmp,.gif,image/*"
        :formats="$t('image_editor.supported_formats')"
        icon="image"
        @select="onFileSelect($event[0]!)"
      />

      <div v-if="selectedFile" class="start-blank-wrap">
        <v-filled-button @click="startEditing">{{ $t('image_editor.start_editing') }}</v-filled-button>
      </div>

      <div class="start-blank-wrap">
        <button class="start-blank" @click="startBlank">{{ $t('image_editor.start_blank') }}</button>
      </div>

      <div v-if="loading" class="loading"><v-circular-progress indeterminate /></div>
      <template v-else-if="projects.length > 0">
        <h2 class="section-title">{{ $t('image_editor.recent_projects') }}</h2>
        <div class="grid">
          <div
            v-for="p in projects"
            :key="p.id"
            class="card"
            @click="openProject(p.id)"
          >
            <div class="thumb checkerboard">
              <img
                v-if="p.previewDataUrl"
                :src="p.previewDataUrl"
                alt=""
                class="thumb-img"
              />
              <i-lucide-image v-else class="thumb-placeholder" />
            </div>
            <div class="info">
              <span>{{ formatTimeAgo(new Date(p.updatedAt).toISOString()) }}</span>
            </div>
            <button
              :id="`project-delete-${p.id}`"
              v-tooltip="$t('image_editor.delete')"
              class="delete-btn"
              @click.stop="openDeleteMenu(p)"
            >
              <i-lucide-trash-2 />
            </button>
            <v-dropdown-menu v-model="deleteMenuVisible[p.id]" :anchor="`project-delete-${p.id}`">
              <inline-delete-confirm
                :loading="deletingId === p.id"
                message-key="image_editor.delete_project_confirm"
                @confirm="handleDelete(p)"
                @cancel="closeDeleteMenu(p.id)"
              />
            </v-dropdown-menu>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, onActivated, onDeactivated } from 'vue'
import { PlainAppProjectStore } from './store/plain-app-store'
import type { ProjectSummary } from './store/project-store'
import { formatTimeAgo } from '@/lib/format'
import ImageDropZone from './components/ImageDropZone.vue'

const store = new PlainAppProjectStore()
const isActive = ref(false)
const loading = ref(true)
const projects = ref<ProjectSummary[]>([])
const deleteMenuVisible = reactive<Record<string, boolean>>({})
const deletingId = ref<string | null>(null)
const selectedFile = ref<File | null>(null)

async function loadProjects() {
  loading.value = true
  try {
    projects.value = await store.list()
  } finally {
    loading.value = false
  }
}

function onFileSelect(file: File) {
  selectedFile.value = file
}

function startEditing() {
  const file = selectedFile.value!
  const url = URL.createObjectURL(file)
  window.open(`/image-editor/new?src=${encodeURIComponent(url)}&name=${encodeURIComponent(file.name)}`, '_blank')
}

function startBlank() {
  window.open('/image-editor/new?blank=true', '_blank')
}

function openProject(id: string) {
  window.open(`/image-editor/${id}`, '_blank')
}

function openDeleteMenu(p: ProjectSummary) {
  deleteMenuVisible[p.id] = true
}

function closeDeleteMenu(id: string) {
  deleteMenuVisible[id] = false
}

async function handleDelete(p: ProjectSummary) {
  if (deletingId.value) return
  deletingId.value = p.id
  try {
    await store.delete(p.id)
    deleteMenuVisible[p.id] = false
    await loadProjects()
  } finally {
    deletingId.value = null
  }
}

onActivated(() => {
  isActive.value = true
  loadProjects()
})

onDeactivated(() => {
  isActive.value = false
})
</script>

<style lang="scss" scoped>
.image-editor-list {
  height: 100%;
  overflow: auto;
}

.header-title {
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--md-sys-color-on-surface);
  margin-left: 8px;
}

.list-content {
  max-width: 800px;
  margin: 0 auto;
  padding: 24px;
}

.loading {
  display: flex;
  justify-content: center;
  padding: 40px 0;
}

.start-blank-wrap {
  display: flex;
  justify-content: center;
  margin-top: 16px;
}

.start-blank {
  background: none;
  border: none;
  color: var(--md-sys-color-primary);
  font-size: 14px;
  cursor: pointer;
  text-decoration: underline;
  padding: 4px 8px;

  &:hover { text-decoration: none; }
}

.section-title {
  font-size: 0.9rem;
  font-weight: 600;
  color: var(--md-sys-color-on-surface-variant);
  margin: 28px 0 12px;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 8px;
}

.card {
  position: relative;
  aspect-ratio: 1 / 1;
  border-radius: 12px;
  overflow: hidden;
  cursor: pointer;
  transition: box-shadow 0.15s;

  &:hover {
    box-shadow: 0 0 0 2px var(--md-sys-color-primary);
    .delete-btn { opacity: 1; }
  }
}

.checkerboard {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--md-sys-color-surface-container-low);
  background-image:
    linear-gradient(45deg, color-mix(in srgb, var(--md-sys-color-outline) 12%, transparent) 25%, transparent 25%),
    linear-gradient(-45deg, color-mix(in srgb, var(--md-sys-color-outline) 12%, transparent) 25%, transparent 25%),
    linear-gradient(45deg, transparent 75%, color-mix(in srgb, var(--md-sys-color-outline) 12%, transparent) 75%),
    linear-gradient(-45deg, transparent 75%, color-mix(in srgb, var(--md-sys-color-outline) 12%, transparent) 75%);
  background-size: 10px 10px;
  background-position: 0 0, 0 5px, 5px -5px, -5px 0;
}

.thumb-img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.thumb-placeholder {
  font-size: 24px;
  color: var(--md-sys-color-on-surface-variant);
}

.info {
  position: absolute;
  bottom: 0;
  right: 0;
  padding: 4px 8px;
  background: rgba(0, 0, 0, 0.4);
  border-radius: 12px 0 12px 0;
  color: #fff;
  font-size: 12px;
  z-index: 1;
}

.delete-btn {
  position: absolute;
  top: 6px;
  right: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 50%;
  background: rgba(0, 0, 0, 0.4);
  color: #fff;
  cursor: pointer;
  font-size: 13px;
  opacity: 0;
  transition: opacity 0.15s, background 0.15s;
  z-index: 2;

  &:hover { background: var(--md-sys-color-error); }
}
</style>
