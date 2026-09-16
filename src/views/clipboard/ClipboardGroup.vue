<template>
  <peer-group-shell
    :name="group.name"
    :device-type="group.deviceType"
    :online="group.online"
    :count="group.total"
    :clearable="group.online && group.items.length > 0"
    @clear="$emit('clear')"
  >
    <template #actions>
      <v-chip-select
        :model-value="direction"
        :options="directionOptions"
        :icon="IconSettings"
        :aria-label="$t('settings')"
        @update:model-value="pickDirection"
      />
    </template>

    <div v-if="group.items.length" class="grp-items">
      <clipboard-item
        v-for="item in group.items"
        :key="item.id"
        :item="item"
        @delete="deleteItem(group.peerId, [$event.id])"
      />
    </div>
    <NoDataPlaceholder
      v-else
      :loading="group.loading"
      :online="group.online"
      :peer-id="group.peerId"
      :placeholder-key="group.clipboardSync === false ? 'no_permission' : ''"
      feature="CLIPBOARD"
    />
    <v-pagination
      v-if="group.total > limit"
      :page="group.page"
      :go="(p: number) => fetchPage(group.peerId, p)"
      :total="group.total"
      :limit="limit"
    />
  </peer-group-shell>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import IconSettings from '~icons/material-symbols/settings-outline-rounded'
import PeerGroupShell from '@/components/PeerGroupShell.vue'
import ClipboardItem from '@/components/ClipboardItem.vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import VChipSelect from '@/components/base/VChipSelect.vue'
import type { VSelectOption } from '@/components/base/VSelect.vue'
import {
  peerClipboardDirection,
  setPeerClipboardDirection,
  type ClipboardDirection,
  type PeerClipboardGroup,
} from '@/lib/peer/local-clipboard-data'
import { useLocalClipboardActions } from './local-clipboard'

const props = defineProps<{
  group: PeerClipboardGroup
}>()

defineEmits<{
  clear: []
}>()

const { t } = useI18n()

const { limit, deleteItem, fetchPage } = useLocalClipboardActions()

const directionOptions = computed<VSelectOption[]>(() => [
  { value: 'off', label: t('clipboard_direction_off') },
  { value: 'push', label: t('clipboard_direction_push') },
  { value: 'pull', label: t('clipboard_direction_pull') },
  { value: 'both', label: t('clipboard_direction_both') },
])

const direction = computed(() => peerClipboardDirection(props.group.peerId))

function pickDirection(value: string | number) {
  setPeerClipboardDirection(props.group.peerId, value as ClipboardDirection)
}
</script>

<style lang="scss" scoped>
.grp-items {
  display: contents;
}
</style>
