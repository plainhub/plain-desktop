<template>
  <header
    v-if="current"
    class="toolbar"
    :class="{ 'toolbar--popup': popup }"
    :data-tauri-drag-region="isTauri"
  >
    <div v-if="current.name && !popup" class="source-name">
      <v-icon-button v-if="!isTauri" v-tooltip="$t('back')" class="close-btn" @click="$emit('close')">
        <i-material-symbols:arrow-back-rounded />
      </v-icon-button>
      <div class="name-block">
        <span v-if="!isTauri" class="file-name">{{ current.name }}</span>
        <span v-if="transcoded && isVideo(current.name)" class="codec-tip">
          {{ $t('video_transcode_tip') }}
          <a :href="hevcHelpUrl" target="_blank" rel="noopener" class="codec-tip-link">{{ $t('video_transcode_help') }}</a>
        </span>
      </div>
    </div>

    <div class="actions">
      <template v-if="!popup && isTauri">
        <v-icon-button v-tooltip="$t('back')" class="close-btn" @click="$emit('close')">
          <i-material-symbols:arrow-back-rounded />
        </v-icon-button>
        <v-icon-button v-tooltip="$t('open_in_window')" @click="$emit('open-in-window')">
          <i-material-symbols:open-in-new-rounded />
        </v-icon-button>
      </template>
      <template v-if="isImage(current.name)">
        <LightboxQualityDropdown :model-value="imageQuality" @update:model-value="$emit('update:image-quality', $event)" />

        <v-icon-button v-if="!readOnly" v-tooltip="$t('edit_image')" @click="$emit('edit-image')">
          <i-material-symbols:edit-rounded />
        </v-icon-button>

        <v-icon-button v-tooltip="$t('zoom_in')" @click="$emit('zoom-in')">
          <i-material-symbols:zoom-in-rounded />
        </v-icon-button>

        <v-icon-button v-tooltip="$t('zoom_out')" @click="$emit('zoom-out')">
          <i-material-symbols:zoom-out-rounded />
        </v-icon-button>

        <v-icon-button v-tooltip="$t('resize')" @click="$emit('resize')">
          <i-material-symbols:aspect-ratio-outline-rounded />
        </v-icon-button>

        <v-icon-button v-tooltip="$t('rotate_left')" @click="$emit('rotate-left')">
          <i-material-symbols:rotate-left-rounded />
        </v-icon-button>

        <v-icon-button v-tooltip="$t('rotate_right')" @click="$emit('rotate-right')">
          <i-material-symbols:rotate-right-rounded />
        </v-icon-button>
      </template>
    </div>

    <v-icon-button v-tooltip="$t('info')" class="info-btn" @click="$emit('toggle-info')">
      <i-material-symbols:info-outline-rounded />
    </v-icon-button>
  </header>
</template>

<script setup lang="ts">
import { isImage, isVideo } from '@/lib/file'
import { HEVC_HELP_URL as hevcHelpUrl } from '@/lib/video-codec'
import type { ISource } from './types'

const isTauri = __IS_TAURI__

defineProps<{
  current: ISource | undefined
  popup?: boolean
  readOnly?: boolean
  transcoded?: boolean
  imageQuality: 'fast' | 'original'
}>()

defineEmits<{
  close: []
  'zoom-in': []
  'zoom-out': []
  resize: []
  'rotate-left': []
  'rotate-right': []
  'toggle-info': []
  'open-in-window': []
  'edit-image': []
  'update:image-quality': [value: 'fast' | 'original']
}>()
</script>

<style lang="scss" scoped>
.toolbar {
  display: flex;
  flex-direction: row;
  padding: 8px 12px;
  align-items: center;
  background: var(--md-sys-color-surface);
  z-index: 1;
  position: static;
  width: 100%;
  box-sizing: border-box;
  grid-area: toolbar;
  min-height: 56px;

  .source-name {
    flex: 1;
    display: flex;
    align-items: center;
    min-width: 0; /* Allow text truncation */
    margin-right: 8px;

    .close-btn {
      margin-right: 8px;
      flex-shrink: 0;
    }

    .name-block {
      min-width: 0;
      display: flex;
      flex-direction: column;
      gap: 2px;
    }

    .file-name {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-size: 14px;
    }

    .codec-tip {
      font-size: 12px;
      line-height: 1.35;
      color: var(--md-sys-color-on-surface-variant);
      overflow: hidden;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      -webkit-box-orient: vertical;
    }

    .codec-tip-link {
      color: var(--md-sys-color-primary);
      text-decoration: none;
      white-space: nowrap;

      &:hover {
        text-decoration: underline;
      }
    }
  }

  .actions {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    flex-wrap: wrap;
    gap: 4px;
  }

  &.toolbar--popup {
    padding-left: 80px; /* leave room for macOS traffic lights */
    -webkit-app-region: drag;

    .actions {
      margin-left: auto;
    }
  }

  .info-btn {
    width: 40px;
    height: 40px;
    margin: 0;
    flex-shrink: 0;
  }

  @media (max-width: 480px) {
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-rows: auto auto;
    grid-template-areas: 
      "source-name info-btn"
      "actions actions";
    gap: 8px;
    align-items: center;

    .source-name {
      grid-area: source-name;
      margin-right: 0;
    }

    .actions {
      grid-area: actions;
      justify-self: center;
    }

    .info-btn {
      grid-area: info-btn;
      justify-self: end;
    }
  }
}
</style>
