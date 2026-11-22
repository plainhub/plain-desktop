<template>
  <div class="actions">
    <v-icon-button class="sm" v-tooltip="$t('restore')" :aria-label="$t('restore')" :disabled="busy" @click.stop="emit('restore')">
      <i-material-symbols:restore-from-trash-outline-rounded />
    </v-icon-button>
    <v-icon-button :id="anchor" class="sm" v-tooltip="$t('delete')" :aria-label="$t('delete')" :disabled="busy" @click.stop="emit('delete', anchor)">
      <i-material-symbols:delete-forever-outline-rounded />
    </v-icon-button>
  </div>
</template>

<script setup lang="ts">
import { useId } from 'vue'

defineProps<{ busy: boolean }>()
const emit = defineEmits<{ restore: []; delete: [anchor: string] }>()
const anchor = useId()
</script>
