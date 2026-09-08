<template>
  <div class="quick-content-main">
    <div class="top-app-bar">
      <button v-tooltip="$t('close')" class="btn-icon" @click.prevent="store.quick = ''">
        <i-material-symbols:arrow-back-rounded />
      </button>
      <div class="title">
        {{ $t('header_actions.notifications') }} ({{ notifications.length }})
        <div v-if="hasNotificationWarning" class="warning-indicator">
          <v-dropdown v-model="warningMenuVisible">
            <template #trigger>
              <button class="btn-icon warning-icon">
                <i-material-symbols:warning-outline />
              </button>
            </template>
            <div class="warning-dropdown">
              <div class="warning-content">
                <i-material-symbols:error-outline-rounded />
                <div class="warning-text">
                  {{ $t(notificationWarningMessage) }}
                </div>
              </div>
              <div v-if="notificationWarningAction" class="warning-actions">
                <v-filled-button class="btn-sm" @click="notificationWarningAction.action()">
                  {{ $t(notificationWarningAction.text) }}
                </v-filled-button>
              </div>
            </div>
          </v-dropdown>
        </div>
      </div>
      <div class="actions">
        <notification-sound-button v-model="notificationVolume" />
        <button v-if="notifications.length" v-tooltip="$t('clear_list')" class="btn-icon" @click.prevent="clearAll">
          <i-material-symbols:delete-forever-outline-rounded />
        </button>
      </div>
    </div>

    <div class="quick-content-body">
      <section v-if="notifications.length" class="list-items">
        <notification-item
          v-for="item in notifications"
          :key="item.id"
          :item="item"
          :replying="replyingId === item.id"
          :sending="replySending"
          @reply="startReply(item.id, $event)"
          @cancel-reply="cancelReply"
          @send="sendReply(item.id, $event)"
          @delete="deleteItem(item)"
        />
      </section>
      <NoDataPlaceholder v-else :loading="loading" :permissions="app.permissions" permission="NOTIFICATION_LISTENER" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import NoDataPlaceholder from '@/components/NoDataPlaceholder.vue'
import NotificationSoundButton from '@/components/NotificationSoundButton.vue'
import NotificationItem from '@/components/NotificationItem.vue'
import { useNotifications } from './notifications'

const warningMenuVisible = ref(false)

const {
  store, app, notificationVolume, notifications, loading,
  hasNotificationWarning, notificationWarningMessage, notificationWarningAction,
  replyingId, replySending,
  startReply, cancelReply, sendReply, deleteItem, clearAll,
} = useNotifications()
</script>
