<template>
  <div class="quick-content-main">
    <div class="top-app-bar">
      <button v-tooltip="$t('close')" class="btn-icon" @click.prevent="store.quick = ''">
        <i-material-symbols:arrow-back-rounded />
      </button>
      <div class="title">
        {{ $t('header_actions.notifications') }}
        <span v-if="total" class="count-pill">{{ total }}</span>
      </div>
      <div class="actions">
        <notification-sound-button v-model="notificationVolume" />
        <button v-if="total" v-tooltip="$t('clear_list')" class="btn-icon" @click.prevent="clearAll">
          <i-material-symbols:delete-forever-outline-rounded />
        </button>
      </div>
    </div>

    <div class="quick-content-body">
      <notification-group
          v-for="g in groups"
          :key="g.peerId"
          :group="g"
          @clear="clearGroup(g.peerId)"
        >
          <div v-if="g.items.length" class="grp-items">
            <notification-item
              v-for="item in g.items"
              :key="item.id"
              :item="item"
              :replying="replyingId === replyKey(g.peerId, item.id)"
              :sending="replySending"
              :deletable="g.online"
              @reply="startReply(g.peerId, item.id, $event)"
              @cancel-reply="cancelReply"
              @send="sendReply(g.peerId, item.id, $event)"
              @delete="deleteItem(g.peerId, item.id)"
            />
          </div>
          <NoDataPlaceholder v-else :loading="g.loading" :online="g.online" :peer-id="g.peerId" :permissions="g.permissions" permission="NOTIFICATION_LISTENER" />
      </notification-group>
      <NoDataPlaceholder v-if="!groups.length" :loading="groups.some((g) => g.loading)" />
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

const replyKey = (peerId: string, id: string) => `${peerId}:${id}`

const {
  groups, total,
  replyingId, replySending,
  startReply, cancelReply, sendReply,
  deleteItem, clearGroup, clearAll,
} = useLocalNotifications()
</script>

<style lang="scss" scoped>
.quick-content-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
  .peer-group:first-child {
    margin-top: 16px;
  }
}

.grp-items {
  display: contents;
}
</style>
