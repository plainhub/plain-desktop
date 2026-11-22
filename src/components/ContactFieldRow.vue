<template>
  <div class="form-row">
    <div class="field-type"><slot name="selector" /></div>
    <v-text-field v-model="value" class="field-value" :label="label" :placeholder="placeholder" />
    <button v-if="removable" type="button" class="btn-icon" :aria-label="$t('delete')" @click="$emit('remove')">
      <i-material-symbols:delete-forever-outline-rounded />
    </button>
  </div>
</template>

<script setup lang="ts">
const value = defineModel<string>({ required: true })
defineProps<{ label?: string; placeholder?: string; removable?: boolean }>()
defineEmits<{ remove: [] }>()
</script>

<style scoped>
.field-type {
  flex: 0 1 auto;
  max-inline-size: 40%;
}

.field-value {
  min-inline-size: 0;
}
</style>
