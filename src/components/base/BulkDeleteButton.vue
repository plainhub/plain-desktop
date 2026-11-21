<template>
  <v-icon-button :id="btnId" v-tooltip="$t('delete')" :loading="loading" @click.stop="$emit('click')">
    <i-material-symbols:delete-forever-outline-rounded />
  </v-icon-button>
  <delete-confirm-popup
    :model-value="confirming" :anchor="btnId" :count="count" :loading="loading"
    @update:model-value="!$event && $emit('cancel')" @confirm="$emit('confirm')"
  />
</template>

<script setup lang="ts">
import { useId } from 'vue'

defineProps<{ count: number; loading?: boolean; confirming: boolean }>()
defineEmits<{ click: []; confirm: []; cancel: [] }>()

const btnId = useId()
</script>
