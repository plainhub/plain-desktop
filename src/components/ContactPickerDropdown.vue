<template>
  <!--
    Wrap the entire input field (default slot) so the dropdown anchors
    to the full input width, not just the icon button inside it.
    @click.stop on the inner wrapper prevents field clicks from toggling.
    The toggle function is exposed via slot props for the icon button.
  -->
  <v-dropdown :model-value="modelValue" strategy="below" :full-width="true" :max-height="280" @update:model-value="$emit('update:modelValue', $event)">
    <template #trigger>
      <div @click.stop>
        <slot :toggle="toggle" />
      </div>
    </template>
    <div v-if="loading" class="contact-picker-loading">
      <v-circular-progress indeterminate class="sm" />
    </div>
    <template v-else>
      <template v-for="contact in contacts" :key="contact.id">
        <div
          v-for="(phone, pi) in contact.phoneNumbers"
          :key="pi"
          class="dropdown-item contact-picker-item"
          @click="$emit('select', phone.normalizedNumber || phone.value, contact)"
        >
          <div class="contact-picker-info">
            <span class="contact-picker-name">{{ getContactFullName(contact) }}</span>
            <span v-if="contact.phoneNumbers.length > 1 && phone.type !== 'CUSTOM'" class="contact-picker-type">
              {{ $t(`contact.phone_number_type.${phone.type}`) }}
            </span>
          </div>
          <span class="contact-picker-number">{{ phone.normalizedNumber || phone.value }}</span>
        </div>
      </template>
    </template>
  </v-dropdown>
</template>

<script setup lang="ts">
import { getContactFullName } from '@/lib/contact/format'
import type { IContact } from '@/lib/interfaces'

const props = defineProps<{
  modelValue: boolean
  contacts: IContact[]
  loading: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: boolean]
  select: [phone: string, contact: IContact]
}>()

function toggle() {
  emit('update:modelValue', !props.modelValue)
}
</script>

<style scoped lang="scss">
.contact-picker-loading {
  display: flex;
  justify-content: center;
  padding: 20px;
}

.contact-picker-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-width: 280px;
}

.contact-picker-info {
  display: flex;
  flex-direction: column;
  gap: 1px;
  flex: 1;
  min-width: 0;
  overflow: hidden;
}

.contact-picker-name {
  font-size: 0.875rem;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.contact-picker-type {
  font-size: 0.6875rem;
  color: var(--md-sys-color-on-surface-variant);
}

.contact-picker-number {
  font-size: 0.8125rem;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
}
</style>
