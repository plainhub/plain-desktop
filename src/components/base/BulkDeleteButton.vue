<template>
  <v-icon-button :id="btnId" v-tooltip="$t('delete')" :loading="loading" @click.stop="$emit('click')">
    <i-material-symbols:delete-forever-outline-rounded />
  </v-icon-button>
  <v-dropdown-menu v-model="open" :anchor="btnId">
    <inline-delete-confirm :count="count" :loading="loading" @confirm="$emit('confirm')" @cancel="$emit('cancel')" />
  </v-dropdown-menu>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'

const props = defineProps<{ count: number; loading?: boolean; confirming: boolean }>()
const emit = defineEmits<{ click: []; confirm: []; cancel: [] }>()

const btnId = `bulk-delete-${Math.random().toString(36).slice(2, 8)}`
const open = ref(false)

watch(() => props.confirming, (val) => { open.value = val })
watch(open, (val) => { if (!val && props.confirming) emit('cancel') })
</script>
