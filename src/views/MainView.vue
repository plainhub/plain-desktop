<template>
  <div class="main-view-root">
    <tauri-tab-bar v-if="isTauri" />
    <v-modal v-if="errorMessage" @close="errorMessage = ''">
      <template #headline>{{ $t('error') }}</template>
      <template #content>{{ $t(errorMessage) }}</template>
      <template #actions>
        <v-filled-button @click="errorMessage = ''">{{ $t('ok') }}</v-filled-button>
      </template>
    </v-modal>
    <div class="layout">
      <header id="header">
        <section class="start">
          <v-icon-button
            v-if="hasLeftSidebar"
            v-tooltip="$t(store.miniSidebar ? 'open' : 'close')"
            class="sidebar-toggle"
            @click.prevent="toggleSidebar"
          >
            <i-material-symbols:left-panel-open-outline-rounded v-if="store.miniSidebar" />
            <i-material-symbols:left-panel-close-outline-rounded v-else />
          </v-icon-button>
          <div id="header-start-slot"></div>
        </section>
        <section class="end">
          <div id="header-end-slot"></div>
          <HeaderSearch v-if="showHeaderSearch" :placeholder="$t('search')" />
        </section>
      </header>
      <app-rail />
      <div class="page-content">
        <!-- The cache key $route.meta.group is mainly used for MediaSidebar, otherwise the component will be cached totally. -->
        <router-view v-if="appReady" v-slot="{ Component }" name="LeftSidebar">
          <keep-alive :max="20">
            <component :is="Component" :key="$route.meta.group" />
          </keep-alive>
        </router-view>
        <!-- Mobile sidebar backdrop -->
        <div 
          v-if="hasLeftSidebar && appReady"
          class="sidebar-backdrop" 
          :class="{ visible: !store.miniSidebar && isTablet }"
          @click="store.miniSidebar = true"
        ></div>
        <main v-if="appReady" class="main" :class="'main-' + ($route.meta.className || 'default')">
          <router-view v-slot="{ Component }" name="LeftSidebar2">
            <keep-alive :max="12">
              <component :is="Component" :key="getSidebar2CacheKey()" />
            </keep-alive>
          </router-view>
          <router-view v-slot="{ Component }">
            <keep-alive exclude="NoteEditView" :max="10">
              <component :is="Component" :key="$route.fullPath" />
            </keep-alive>
          </router-view>
        </main>
        <div v-if="!appReady" class="loading-overlay">
          <v-circular-progress indeterminate />
        </div>
      </div>
        <div v-if="appReady" class="quick-actions">
            <template v-if="!localMode">
                <v-icon-button
                  v-if="hasTasks || store.quick === 'upload'"
                  v-tooltip="$t('header_actions.uploads')"
                  class="q-action"
                  toggle
                  :class="{ selected: store.quick === 'upload' && !hasActiveUploads, uploading: hasActiveUploads }"
                  @click="toggleQuick('upload')"
                >
                  <span class="upload-action-icon">
                    <i-material-symbols:format-list-numbered-rounded />
                  </span>
                </v-icon-button>
                <v-icon-button
                  v-if="app.channel !== AppChannelType.GOOGLE"
                  v-tooltip="$t('header_actions.notifications')"
                  class="q-action"
                  toggle
                  :class="{ selected: store.quick === 'notification' }"
                  @click="toggleQuick('notification')"
                >
                  <i-material-symbols:notifications-outline-rounded />
                </v-icon-button>
                <v-icon-button id="quick-audio" v-tooltip="$t('playlist')" class="q-action" toggle :class="{ selected: store.quick === 'audio' }" @click="toggleQuick('audio')">
                  <i-material-symbols:queue-music-rounded />
                </v-icon-button>
                <v-icon-button v-tooltip="$t('pomodoro_timer')" class="q-action" toggle :class="{ selected: store.quick === 'pomodoro' }" @click="toggleQuick('pomodoro')">
                  <i-material-symbols:timer-outline />
                </v-icon-button>
            </template>
            <v-icon-button
              v-if="localMode"
              v-tooltip="$t('header_actions.notifications')"
              class="q-action"
              toggle
              :class="{ selected: store.quick === 'notification' }"
              @click="toggleQuick('notification')"
            >
              <i-material-symbols:notifications-outline-rounded />
            </v-icon-button>
            <v-icon-button v-tooltip="$t('bookmarks')" class="q-action" toggle :class="{ selected: store.quick === 'bookmark' }" @click="toggleQuick('bookmark')">
              <i-lucide:bookmark />
            </v-icon-button>
            <div v-show="store.quick" class="drag-indicator" @mousedown="resizeWidth">
              <i-material-symbols:drag-indicator />
            </div>
        </div>
        <transition name="width">
          <div v-if="appReady" v-show="store.quick" class="quick-content" :style="{ width: store.quickContentWidth + 'px' }">
            <upload-list v-show="store.quick === 'upload'" />
            <audio-player v-show="store.quick === 'audio'" />
            <p-notifications v-if="!localMode" v-show="store.quick === 'notification'" />
            <local-notifications v-if="localMode" v-show="store.quick === 'notification'" />
            <pomodoro-timer v-show="store.quick === 'pomodoro'" />
            <bookmark-list v-show="store.quick === 'bookmark'" />
          </div>
        </transition>
      <lightbox />
    </div>
  </div>
</template>

<script setup lang="ts">
import { inject } from 'vue'
import HeaderSearch from '@/components/HeaderSearch.vue'
import BookmarkList from '@/views/bookmarks/BookmarkList.vue'
import LocalNotifications from '@/views/notifications/LocalNotifications.vue'
import { AppChannelType } from '@/lib/status'
import { useMainView } from '@/hooks/main-view'

const isTablet = inject('isTablet')
const isTauri = __IS_TAURI__

const {
  store, app, appReady, errorMessage,
  hasTasks, hasActiveUploads, hasLeftSidebar, showHeaderSearch, localMode,
  toggleSidebar, toggleQuick, getSidebar2CacheKey, resizeWidth,
} = useMainView()
</script>

<style lang="scss" scoped>
.main-view-root {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;
}

.layout {
  display: grid;
  grid-template-areas:
    'rail head quick-actions quick-content'
    'rail page-content quick-actions quick-content';
  grid-template-columns: auto 1fr auto auto;
  grid-template-rows: auto 1fr;
  flex: 1;
  min-height: 0;

}

.page-content {
  grid-area: page-content;
  display: flex;
  min-height: 0;
  position: relative;

  .loading-overlay {
    position: absolute;
    inset: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 16px 16px 0 0;
    background: color-mix(in srgb, var(--md-sys-color-surface) 70%, transparent);
  }
}

.quick-actions {
  display: flex;
  flex-direction: column;
  height: 100%;
  position: relative;
  width: 56px;
  grid-area: quick-actions;

  .q-action {
    margin: 8px;

    &.uploading {
      color: var(--md-sys-color-primary);

      .upload-action-icon {
        position: relative;
        display: inline-flex;
        align-items: center;
        justify-content: center;

        &::before {
          content: '';
          position: absolute;
          width: 34px;
          height: 34px;
          border-radius: 50%;
          border: 2.5px solid transparent;
          border-top-color: var(--md-sys-color-primary);
          border-right-color: var(--md-sys-color-primary);
          animation: upload-ring-spin 0.85s linear infinite;
        }
      }
    }
  }

  .drag-indicator {
    align-self: center;
    margin-top: auto;
    margin-bottom: auto;
    cursor: col-resize;
  }
}

@keyframes upload-ring-spin {
  to { transform: rotate(360deg); }
}

.quick-content {
  grid-area: quick-content;
  overflow: hidden;
}

#header {
  align-items: center;
  height: var(--pl-top-app-bar-height);
  grid-area: head;
  inset: 0 0 auto 0;
  display: flex;
  box-sizing: border-box;
  color: var(--md-sys-color-on-surface);

  .start {
    margin-inline-start: 8px;
    flex: 1;
    box-sizing: border-box;
    overflow: auto;
    scroll-behavior: smooth;
    display: flex;
    align-items: center;
    /* Hide the scrollbars */
    scrollbar-width: none;
    /* Firefox */
    -ms-overflow-style: none;

    /* Internet Explorer/Edge */
    &::-webkit-scrollbar {
      display: none;
      /* Chrome/Safari/Opera */
    }
  }

  .end {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    margin-inline-end: 8px;
  }

  #header-start-slot,
  #header-end-slot {
    display: contents;
  }
}

@media (max-width: 768px) {
  .layout {
    grid-template-areas:
      'head'
      'page-content'
      'rail';
    grid-template-columns: 1fr;
    grid-template-rows: auto 1fr auto;
  }
}

</style>
