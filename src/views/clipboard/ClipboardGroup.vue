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
      <v-dropdown v-model="settingsOpen" strategy="below">
        <template #trigger>
          <button v-tooltip="$t('settings')" class="btn-icon gear-btn">
            <i-material-symbols:settings-outline-rounded />
          </button>
        </template>
        <div
          v-for="opt in directionOptions"
          :key="opt.value"
          class="dropdown-item"
          :class="{ selected: direction === opt.value }"
          @click="pickDirection(opt.value)"
        >
          <span>{{ $t(opt.labelKey) }}</span>
          <i-material-symbols:check-rounded v-if="direction === opt.value" class="dropdown-item-icon" />
        </div>
      </v-dropdown>
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
      :placeholder-key="placeholderKey"
      :show-settings-link="placeholderKey === 'clipboard_sync_disabled'"
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
import { computed, ref } from 'vue'
import PeerGroupShell from '@/components/PeerGroupShell.vue'
import ClipboardItem from '@/components/ClipboardItem.vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
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

const { limit, deleteItem, fetchPage } = useLocalClipboardActions()

const settingsOpen = ref(false)

const directionOptions: Array<{ value: ClipboardDirection; labelKey: string }> = [
  { value: 'off', labelKey: 'clipboard_direction_off' },
  { value: 'push', labelKey: 'clipboard_direction_push' },
  { value: 'pull', labelKey: 'clipboard_direction_pull' },
  { value: 'both', labelKey: 'clipboard_direction_both' },
]

const direction = computed(() => peerClipboardDirection(props.group.peerId))

const placeholderKey = computed(() => {
  // Offline dominates: an unreachable device must not advertise feature states.
  if (!props.group.online) return ''
  if (direction.value === 'off' || direction.value === 'push') return 'clipboard_sync_direction_off'
  if (props.group.clipboardSync === false) return 'clipboard_sync_disabled'
  return ''
})

function pickDirection(value: ClipboardDirection) {
  settingsOpen.value = false
  setPeerClipboardDirection(props.group.peerId, value)
}
</script>

<style lang="scss" scoped>
.gear-btn {
  width: 32px;
  height: 32px;

  svg {
    width: 16px;
    height: 16px;
  }
}

.grp-items {
  display: contents;
}
</style>
