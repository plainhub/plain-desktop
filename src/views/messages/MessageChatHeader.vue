<template>
  <div class="chat-header">
    <v-icon-button v-tooltip="$t('back')" @click.stop="$emit('back')">
      <i-material-symbols:arrow-back-rounded />
    </v-icon-button>
    <div class="chat-header-info">
      <span class="chat-header-name">{{ name }}</span>
      <span v-if="address" class="chat-header-address">{{ address }}</span>
    </div>
    <div class="chat-header-actions">
      <v-dropdown v-if="showNotificationWarning" v-model="warnOpen">
        <template #trigger>
          <v-icon-button class="notification-warning-icon">
            <i-material-symbols:warning-outline />
          </v-icon-button>
        </template>
        <div class="notification-warning-tip">
          {{ $t('sms_notification_warning') }}
        </div>
      </v-dropdown>
      <v-icon-button v-tooltip="$t('export_sms')" @click.stop="$emit('export')">
        <i-material-symbols:download-rounded />
      </v-icon-button>
      <v-icon-button v-tooltip="$t('review_mode')" @click.stop="$emit('review')">
        <i-material-symbols:reviews-outline-rounded />
      </v-icon-button>
      <v-icon-button v-tooltip="$t('archive_conversation')" @click.stop="$emit('archive')">
        <i-material-symbols:archive-outline-rounded />
      </v-icon-button>
      <v-icon-button v-tooltip="$t('call')" @click.stop="$emit('call')">
        <i-material-symbols:call-outline-rounded />
      </v-icon-button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
defineProps<{
  name: string
  address: string
  showNotificationWarning: boolean
}>()

defineEmits<{
  back: []
  export: []
  archive: []
  call: []
  review: []
}>()

const warnOpen = ref(false)
</script>

<style scoped lang="scss">
.chat-header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 16px;
  border-bottom: 1px solid var(--md-sys-color-outline-variant);
  min-height: 56px;
  box-sizing: border-box;
}

.chat-header-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.chat-header-name {
  font-weight: 500;
  font-size: 1rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chat-header-address {
  font-size: 0.75rem;
  color: var(--md-sys-color-on-surface-variant);
}

.chat-header-actions {
  display: flex;
  gap: 4px;
}

.notification-warning-icon {
  color: var(--md-sys-color-error);
}

.notification-warning-tip {
  max-width: 280px;
  padding: 12px;
  font-size: 0.85rem;
  line-height: 1.4;
}
</style>
