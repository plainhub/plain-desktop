<template>
  <LeftSidebar v-model:collapsed="collapsed">
    <template #body>
      <div class="section-title">{{ $t('type') }}</div>
      <ul class="nav">
        <SidebarListItem :title="$t('all')" :active="domain === 'all'" tabindex="0" @keydown.enter="selectType()" @click="selectType()">
          <template #start><i-lucide:layout-grid /></template>
        </SidebarListItem>
        <SidebarListItem v-for="kind in searchDomains" :key="kind" :title="$t(`page_title.${kind}`)" :active="domain === kind" tabindex="0" @keydown.enter="selectType(kind)" @click="selectType(kind)">
          <template #start>
            <i-lucide:image v-if="kind === 'images'" />
            <i-lucide:video v-else-if="kind === 'videos'" />
            <i-lucide:file-text v-else-if="kind === 'docs'" />
            <i-lucide:rss v-else-if="kind === 'feeds'" />
            <i-lucide:notebook-pen v-else />
          </template>
        </SidebarListItem>
      </ul>
    </template>
  </LeftSidebar>
</template>

<script setup lang="ts">
import { inject } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import LeftSidebar from '@/components/LeftSidebar.vue'
import SidebarListItem from '@/components/SidebarListItem.vue'
import { searchDomains, type SearchDomain } from '@/lib/api/global-search'
import { globalSearchKey } from '@/hooks/global-search'

const { search, sidebarCollapsed: collapsed } = inject(globalSearchKey)!
const { domain } = search
const route = useRoute()
const router = useRouter()

function selectType(type?: SearchDomain) {
  void router.replace({ name: 'search', query: { ...route.query, type } })
}
</script>
