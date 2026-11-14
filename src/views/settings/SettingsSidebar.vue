<template>
  <left-sidebar>
    <template #body>
      <ul class="nav">
        <SidebarListItem :title="$t('events')" :active="activeSection === 'events'" @click="navigate('/settings/events')">
          <template #start><i-lucide:history /></template>
        </SidebarListItem>
        <SidebarListItem :title="$t('device_info')" :active="activeSection === 'device-info'" @click="navigate('/settings/device-info')">
          <template #start>
            <i-lucide:smartphone />
          </template>
        </SidebarListItem>
        <SidebarListItem v-if="tempStore.app?.debug || tempStore.app?.developerMode" :title="$t('developer.prefs')" :active="activeSection === 'prefs'" @click="navigate('/settings/prefs')">
          <template #start>
            <i-lucide:archive />
          </template>
        </SidebarListItem>
        <SidebarListItem v-if="tempStore.app?.debug || tempStore.app?.developerMode" :title="$t('developer.database')" :active="activeSection === 'database'" @click="navigate('/settings/database')">
          <template #start>
            <i-lucide:database />
          </template>
        </SidebarListItem>
        <SidebarListItem v-if="tempStore.app?.developerMode" :title="$t('developer.logs')" :active="activeSection === 'logs'" @click="navigate('/settings/logs')">
          <template #start>
            <i-lucide:scroll-text />
          </template>
        </SidebarListItem>
        <SidebarListItem :title="$t('customize_ui')" :active="activeSection === 'customize-ui'" @click="navigate('/settings/customize-ui')">
          <template #start>
            <i-lucide:layout-list />
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
import { useTempStore } from '@/stores/temp'

const mainStore = useMainStore()
const tempStore = useTempStore()

const activeSection = computed(() => {
  const path = router.currentRoute.value.path
  if (path.startsWith('/settings/events')) return 'events'
  if (path.startsWith('/settings/device-info')) return 'device-info'
  if (path.startsWith('/settings/prefs')) return 'prefs'
  if (path.startsWith('/settings/database')) return 'database'
  if (path.startsWith('/settings/logs')) return 'logs'
  if (path.startsWith('/settings/customize-ui')) return 'customize-ui'
  return ''
})

function navigate(path: string) {
  replacePath(mainStore, path)
}
</script>
