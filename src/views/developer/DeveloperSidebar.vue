<template>
  <left-sidebar>
    <template #body>
      <ul class="nav">
        <SidebarListItem
          :title="$t('device_info')"
          :active="activeSection === 'device-info'"
          @click="navigate('/developer/device-info')"
        >
          <template #start>
            <i-lucide:smartphone />
          </template>
        </SidebarListItem>
        <SidebarListItem
          :title="$t('developer.prefs')"
          :active="activeSection === 'prefs'"
          @click="navigate('/developer/prefs')"
        >
          <template #start>
            <i-lucide:archive />
          </template>
        </SidebarListItem>
        <SidebarListItem
          :title="$t('developer.database')"
          :active="activeSection === 'database'"
          @click="navigate('/developer/database')"
        >
          <template #start>
            <i-lucide:database />
          </template>
        </SidebarListItem>
        <SidebarListItem
          :title="$t('developer.logs')"
          :active="activeSection === 'logs'"
          @click="navigate('/developer/logs')"
        >
          <template #start>
            <i-lucide:scroll-text />
          </template>
        </SidebarListItem>
      </ul>
    </template>
  </left-sidebar>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import router, { replacePath } from '@/plugins/router'
import { useMainStore } from '@/stores/main'

const mainStore = useMainStore()

const activeSection = computed(() => {
  const path = router.currentRoute.value.path
  if (path.startsWith('/developer/device-info')) return 'device-info'
  if (path.startsWith('/developer/prefs')) return 'prefs'
  if (path.startsWith('/developer/database')) return 'database'
  if (path.startsWith('/developer/logs')) return 'logs'
  return ''
})

function navigate(path: string) {
  replacePath(mainStore, path)
}
</script>
