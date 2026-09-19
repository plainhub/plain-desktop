<template>
  <header class="header">
    <HeaderActions :logged-in="false" />
  </header>
  <h1>PlainApp</h1>
  <div class="login-block">
    <slot />
  </div>
</template>

<script setup lang="ts">
import HeaderActions from './HeaderActions.vue'
</script>

<style lang="scss" scoped>
.header {
  display: flex;
  justify-content: end;
  margin-top: 6px;
}

h1 {
  margin-top: 100px;
  text-align: center;
}

.login-block {
  box-sizing: border-box;
  width: calc(100% - 32px);
  max-width: 400px;
  margin-inline: auto;
  --outlined-field-bg: var(--md-sys-color-surface-variant);
  background-color: var(--md-sys-color-surface-variant);
  border-radius: var(--pl-shape-xl);
  padding: 40px;
}
</style>
