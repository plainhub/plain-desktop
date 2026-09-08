<template>
  <div class="quick-content-main">
    <div class="top-app-bar">
      <button v-tooltip="$t('close')" class="btn-icon" @click.prevent="store.quick = ''">
        <i-material-symbols:arrow-back-rounded />
      </button>
      <div class="title">
        {{ $t('header_actions.notifications') }} ({{ total }})
      </div>
      <div class="actions">
        <notification-sound-button v-model="notificationVolume" />
        <button v-if="total" v-tooltip="$t('clear_list')" class="btn-icon" @click.prevent="clearAll">
          <i-material-symbols:delete-forever-outline-rounded />
        </button>
      </div>
    </div>

    <div class="quick-content-body">
      <section v-if="groups.length" class="ntf-groups">
        <notification-group
          v-for="g in groups"
          :key="g.peerId"
          :group="g"
          @clear="clearGroup(g.peerId)"
          @open-settings="openSettings(g.peerId)"
        >
          <div v-if="g.items.length" class="list-items">
            <notification-item
              v-for="item in g.items"
              :key="item.id"
              :item="item"
              :replying="replyingId === replyId(g.peerId, item.id)"
              :sending="replySending"
              :deletable="g.online"
              @reply="startReply(g.peerId, item.id, $event)"
              @cancel-reply="cancelReply"
              @send="sendReply(g.peerId, item.id, $event)"
              @delete="deleteItem(g.peerId, item.id)"
            />
          </div>
          <div v-else class="g-empty">{{ $t(g.loading ? 'loading' : g.online ? 'no_data' : 'offline') }}</div>
        </notification-group>
      </section>
      <NoDataPlaceholder v-else :loading="groups.some((g) => g.loading)" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { storeToRefs } from 'pinia'
import NotificationGroup from './NotificationGroup.vue'
import NotificationItem from '@/components/NotificationItem.vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import NotificationSoundButton from '@/components/NotificationSoundButton.vue'
import { useMainStore } from '@/stores/main'
import { useLocalNotifications } from './local-notifications'

const store = useMainStore()
const { notificationVolume } = storeToRefs(store)

const replyId = (peerId: string, id: string) => `${peerId}:${id}`

const {
  groups, total,
  replyingId, replySending,
  startReply, cancelReply, sendReply,
  deleteItem, clearGroup, clearAll, openSettings,
} = useLocalNotifications()
</script>

<style lang="scss" scoped>
.ntf-groups {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 8px;
}

.g-empty {
  padding: 12px;
  text-align: center;
  font-size: 0.8rem;
  color: var(--md-sys-color-on-surface-variant);
}
</style>
