<template>
  <Teleport to="body">
    <div v-if="tempStore.lightbox.visible" class="lightbox" @touchmove="preventDefault" @wheel="onWheel">
      <div class="layout">
        <LightboxHeader
          :current="current"
          :popup="popup"
          :read-only="readOnly"
          :transcoded="current?.transcoded"
          :image-quality="imageViewQuality"
          @close="closeDialog"
          @zoom-in="zoomIn"
          @zoom-out="zoomOut"
          @resize="resize"
          @rotate-left="rotateLeft"
          @rotate-right="rotateRight"
          @toggle-info="lightboxInfoVisible = !lightboxInfoVisible"
          @open-in-window="onOpenInWindow"
          @edit-image="onEditImage"
          @update:image-quality="imageViewQuality = $event"
        />
        <section class="content" @click.self="onBackdropClick">
          <div v-if="tempStore.lightbox.sources.length > 1 && (loop || imgIndex > 0)" class="btn-prev" @click="onPrev">
            <i-material-symbols:chevron-left-rounded />
          </div>
          <div v-if="tempStore.lightbox.sources.length > 1 && (loop || imgIndex < tempStore.lightbox.sources.length - 1)" class="btn-next" @click="onNext">
            <i-material-symbols:chevron-right-rounded />
          </div>
          <div v-if="status.loading" class="loading">
            <v-circular-progress indeterminate />
          </div>
          <div v-else-if="status.loadError" class="v-on-error">
            {{ $t('load_failed', { name: current?.name }) }}
          </div>
          <div v-if="current && isVideo(current.name)" v-show="!status.loading && !status.loadError" class="v-video-wrapper" @click.self="onBackdropClick">
            <video ref="video" controls autoplay="true" :src="current.playbackUrl || current.src" @error="onError" @canplay="onLoad" @playing="onPlaying" @pause="onPause" @volumechange="onVolumeChange" />
          </div>
          <div v-else-if="current && isAudio(current.name)" v-show="!status.loading && !status.loadError" class="v-audio-wrapper" @click.self="onBackdropClick">
            <div style="padding: 50px">
              <audio controls autoplay="true" :src="current.src" @error="onError" @canplay="onLoad" />
            </div>
          </div>
          <div v-else-if="current && isImage(current.name)" v-show="!status.loading && !status.loadError" class="v-img-wrapper" :style="imgWrapperStyle">
            <img
              ref="imgRef"
              draggable="false"
              class="v-img"
              :style="isSvg(current.name) ? 'min-width: ' + imgState.width + 'px;' : ''"
              :src="imgSrc"
              @mousedown="onMouseDown"
              @mouseup="onMouseUp"
              @mousemove="onMouseMove"
              @touchstart="onTouchStart"
              @touchmove="onTouchMove"
              @touchend="onTouchEnd"
              @load="onLoad"
              @error="onError"
              @dblclick="onDblclick"
              @dragstart="
                (e) => {
                  e.preventDefault()
                }
              "
            />
          </div>
        </section>
        
        <!-- Desktop info panel -->
        <LightboxInfo 
          v-if="lightboxInfoVisible && !isPhone && !isTablet" 
          :current="current" 
          :file-info="fileInfo" 
          :url-token-key="urlTokenKey ? urlTokenKey.toString() : ''" 
          :app-dir="app.appDir" 
          :tags-map="tagsMap" 
          :os-version="app.osVersion"
          :read-only="readOnly"
          :download-file="downloadFile"
          @rename-file="renameFile"
          @delete-file="deleteFile"
          @refetch-info="refetchInfo"
        />
      </div>
      
      <!-- Mobile info bottom sheet -->
      <BottomSheet v-if="isPhone || isTablet" v-model="lightboxInfoVisible" :title="$t('info')" show-footer>
        <!-- File Details Section -->
        <LightboxFileDetails 
          :current="current" 
          :file-info="fileInfo" 
          :app-dir="app.appDir" 
        />
        
        <!-- File Tags Section -->
        <LightboxFileTags 
          :current="current" 
          :file-info="fileInfo"
          :tags-map="tagsMap"
        />
        
        <!-- Action Buttons in Footer -->
        <template #footer>
          <LightboxFileActionButtons 
            :current="current" 
            :os-version="app.osVersion"
            :read-only="readOnly"
            :download-file="downloadFile"
            @rename-file="renameFile"
            @delete-file="deleteFile"
            @action-success="handleActionSuccess"
          />
        </template>
      </BottomSheet>
    </div>
  </Teleport>
</template>
<script setup lang="ts">
import { computed, inject, toRef } from 'vue'
import { preventDefault } from './utils/index'
import { isVideo, isImage, isAudio, isSvg } from '@/lib/file'
import { openMediaInWindow } from '@/lib/api/tauri-window'
import {
  useLightboxState,
  useLightboxQueries,
  useLightboxTransform,
  useLightboxNavigation,
  useLightboxFileActions,
  useLightboxEvents,
  useLightboxMouseTouch,
  getImageDisplayUrl,
} from '@/hooks/lightbox'

const props = defineProps({
  loop: { type: Boolean, default: true },
  popup: { type: Boolean, default: false },
})

const emit = defineEmits(['on-error', 'on-prev', 'on-next', 'on-prev-click', 'on-next-click', 'on-index-change'])

const isPhone = inject('isPhone') as boolean
const isTablet = inject('isTablet') as boolean

const {
  tempStore, urlTokenKey, app, lightboxInfoVisible, imageViewQuality,
  imgRef, imgState, setImgSize, imgIndex,
  current, fileInfo, video,
  imgWrapperState, status, imgWrapperStyle,
} = useLightboxState(isPhone)

const { loadInfo, refetchInfo, updateViewOriginImageState, tagsMap, loadTags } =
  useLightboxQueries(current, fileInfo, imgState)

const { zoomIn, zoomOut, rotateLeft, rotateRight, resize, onDblclick, onWheel } =
  useLightboxTransform(imgWrapperState, imgState, status)

const { closeDialog, changeIndex, onNext, onPrev } =
  useLightboxNavigation(tempStore, imgIndex, current, imgWrapperState, status, tagsMap, loadTags, loadInfo, toRef(props, 'loop'), emit as (event: string, ...args: any[]) => void, imageViewQuality)

const readOnly = computed(() => tempStore.lightbox.readOnly)

const { downloadFile, deleteFile, renameFile, handleActionSuccess } =
  useLightboxFileActions(current, fileInfo, tagsMap, urlTokenKey, refetchInfo, isPhone, lightboxInfoVisible)

// HEIC always needs server-side conversion, so it must keep the resize query even in "original" mode.
const imgSrc = computed(() => {
  const s = current.value
  if (!s) return ''
  return getImageDisplayUrl(s, imageViewQuality.value)
})

const { onLoad, onError, onPlaying, onPause, onVolumeChange } =
  useLightboxEvents(tempStore, current, video, status, imgState, imgIndex, setImgSize, refetchInfo, updateViewOriginImageState, closeDialog, onNext, onPrev, changeIndex, emit as (event: string, ...args: any[]) => void, toRef(props, 'popup'))

function onBackdropClick() {
  if (props.popup) return
  closeDialog()
}

async function onOpenInWindow() {
  if (props.popup || !__IS_TAURI__ || !current.value) return
  const source = current.value
  closeDialog()
  await openMediaInWindow(source)
}

function onEditImage() {
  if (!current.value) return
  const src = current.value.src
  const name = current.value.name || ''
  window.open(`/image-editor/new?src=${encodeURIComponent(src)}&name=${encodeURIComponent(name)}`, '_blank')
}

const { onMouseDown, onMouseMove, onMouseUp, onTouchStart, onTouchMove, onTouchEnd } =
  useLightboxMouseTouch(imgWrapperState, imgState, status)
</script>
<style lang="scss" scoped>
.v-on-error {
  position: absolute;
  top: 50%;
  left: 50%;
}

.loading {
  position: absolute;
  top: 50%;
  left: 50%;
  opacity: 0;
  animation: showDiv 0.5s ease-in-out 0.5s forwards;
}

.content {
  grid-area: content;
  position: relative;
  height: calc(100vh - 56px);

  /* Mobile layout adjustment */
  @media (max-width: 480px) {
    height: calc(100vh - 112px); /* Account for two-row header on mobile */
  }
}

.lightbox {
  background: var(--md-sys-color-surface);
  overflow: hidden;
}

.layout {
  display: grid;
  grid-template-areas:
    'toolbar info'
    'content info';
  grid-template-columns: 1fr auto;
  grid-template-rows: auto 1fr;
}

/* Mobile BottomSheet styles */
.lightbox :deep(.bottom-sheet-content) {
  padding-inline: 24px;
  padding-block: 0;
  max-height: 70vh;
  overflow-y: auto;
}

.lightbox :deep(.bottom-sheet-footer) {
  padding: 16px 24px 24px 24px;
  border-top: 1px solid var(--md-sys-color-outline-variant);
}

.v-img-wrapper {
  user-select: none;
  margin: 0;
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50% -50%);
  transition: 0.3s linear;
  will-change: transform opacity;

  img {
    user-select: none;
    user-select: none;
    max-width: 90vw;
    max-height: 90vh;
    display: block;
    position: relative;

    @media (max-width: 750px) {
      max-width: 95vw;
      max-height: 95vh;
    }
  }
}

.v-video-wrapper,
.v-audio-wrapper {
  display: flex;
  justify-content: center;
  align-items: center;
  flex-direction: column;
  height: 100%;

  audio {
    width: 400px;
  }

  video {
    height: 95%;
    max-width: 88%;
  }
}

.btn-prev,
.btn-next {
  user-select: none;
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  cursor: pointer;
  opacity: 0.6;
  font-size: 4rem;
  transition: 0.15s linear;
  outline: none;
  z-index: 1;
  &:hover {
    opacity: 1;
  }
}

.btn-next {
  right: 12px;
}

.btn-prev {
  left: 12px;
}

</style>
