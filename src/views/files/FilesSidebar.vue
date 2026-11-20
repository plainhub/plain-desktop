<template>
  <left-sidebar class="files-sidebar">
    <template #body>
      <ul class="nav">
        <SidebarListItem
          v-for="item in quickLinks"
          :key="item.type"
          :title="item.title"
          :active="item.isChecked"
          @click="openLink(item)"
        >
          <template #start>
            <i-lucide:history v-if="item.type === 'RECENTS'" />
          </template>
        </SidebarListItem>
      </ul>

      <div class="section-title">
        {{ $t('volumes') }}
        <v-icon-button v-if="hasDiskManager" v-tooltip="$t('disk_manager')" class="sm" @click.stop="openDiskManager">
          <i-material-symbols:settings-outline-rounded />
        </v-icon-button>
      </div>
      <div class="volumes">
        <VolumeCard
          v-for="item in volumeLinks" :key="item.fullPath" :title="item.title" :count="item.count || ''" :data="item"
          :used-percent="item.usedPercent || 0" :percent-class="percentClass(item.usedPercent)" :active="item.isChecked"
          :show-progress="item.showProgress" @click="openLink(item)"
        >
          <template v-if="item.type !== DriveType.APP" #actions>
            <v-icon-button
              :id="'volume-' + item.fullPath" v-tooltip="$t('actions')" :aria-label="`${$t('actions')}: ${item.title}`"
              :aria-expanded="volumeMenuVisible && selectedVolume?.fullPath === item.fullPath" class="sm volume-actions"
              @click.prevent.stop="showVolumeMenu(item)" @keydown.enter.stop @keydown.space.stop
            >
              <i-material-symbols:more-horiz />
            </v-icon-button>
          </template>
        </VolumeCard>
      </div>

      <template v-if="favoriteLinks.length">
        <div class="section-title">{{ $t('favorites') }}</div>
        <ul class="nav">
          <SidebarListItem
            v-for="item in favoriteLinks"
            :key="item.fullPath"
            :title="item.title"
            :active="item.isChecked"
            @click="openLink(item)"
          >
            <template #actions>
              <v-icon-button
                :id="'favorite-' + item.fullPath" v-tooltip="$t('actions')" class="sm"
                @click.prevent.stop="showFavoriteMenu(item)"
              >
                <i-material-symbols:more-horiz />
              </v-icon-button>
            </template>
          </SidebarListItem>
        </ul>
      </template>

      <v-dropdown-menu v-model="volumeMenuVisible" :anchor="'volume-' + selectedVolume?.fullPath">
        <div class="dropdown-item" @click="openSetVolumeAlias(); volumeMenuVisible = false">{{ $t('rename') }}</div>
      </v-dropdown-menu>

      <v-dropdown-menu v-model="favoriteMenuVisible" :anchor="'favorite-' + selectedFavorite?.fullPath">
        <div class="dropdown-item" @click="openSetFavoriteAlias(); favoriteMenuVisible = false">{{ $t('rename') }}</div>
        <div class="dropdown-item" @click="removeFavoriteFolder(selectedFavorite!); favoriteMenuVisible = false">
          {{ $t('remove_from_favorites') }}
        </div>
      </v-dropdown-menu>
    </template>
  </left-sidebar>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { storeToRefs } from 'pinia'
import { useTempStore } from '@/stores/temp'
import { Capability } from '@/lib/data'
import { DriveType } from '@/lib/status'
import { hasFeature } from '@/lib/feature'
import VolumeCard from '@/components/storage/VolumeCard.vue'
import DiskManagerModal from '@/components/storage/DiskManagerModal.vue'
import { openModal } from '@/components/modal'
import { useFilesSidebar } from '@/hooks/files-sidebar'

const { app } = storeToRefs(useTempStore())
// Disk formatting is served only by backends that declare the DISK_MANAGER
// capability — the entry keys on app.capabilities, never on the device type.
const hasDiskManager = computed(() => hasFeature(Capability.DISK_MANAGER, app.value?.capabilities))

function openDiskManager() {
  openModal(DiskManagerModal)
}

const {
  quickLinks, volumeLinks, favoriteLinks,
  favoriteMenuVisible, selectedFavorite, volumeMenuVisible, selectedVolume,
  openLink, showFavoriteMenu, removeFavoriteFolder, openSetFavoriteAlias, showVolumeMenu, openSetVolumeAlias, percentClass,
} = useFilesSidebar()
</script>
