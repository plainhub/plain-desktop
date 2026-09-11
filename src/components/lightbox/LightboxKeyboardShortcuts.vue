<template>
  <keyboard-shortcuts :shortcuts="shortcuts" />
</template>

<script setup lang="ts">
const shortcuts = [
  { keys: ['←', '→'], description: 'previous_next' },
  { keys: ['Space'], description: 'play_pause_video' },
  { keys: ['Delete'], description: 'delete_file' },
  { keys: ['modifier', '+', 'Backspace'], description: 'delete_file' },
  { keys: ['modifier', '+', 'Z'], description: 'undo' },
  { keys: ['Esc'], description: 'close_preview' }
]
</script>