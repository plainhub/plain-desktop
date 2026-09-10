<template>
  <section class="ux-section">
    <h2>Buttons</h2>

    <ux-code-block code="<v-filled-button>Primary</v-filled-button>">
      <v-filled-button>Primary</v-filled-button>
      <v-filled-button disabled>Disabled</v-filled-button>
      <v-filled-button :loading="true">Loading</v-filled-button>
    </ux-code-block>

    <ux-code-block code="<v-filled-button>Default</v-filled-button>&#10;<v-filled-button class=&quot;btn-lg&quot;>Large</v-filled-button>">
      <v-filled-button>Default</v-filled-button>
      <v-filled-button class="btn-lg">Large</v-filled-button>
    </ux-code-block>

    <ux-code-block code="<v-outlined-button>Secondary</v-outlined-button>">
      <v-outlined-button>Secondary</v-outlined-button>
      <v-outlined-button disabled>Disabled</v-outlined-button>
      <v-outlined-button :loading="true">Loading</v-outlined-button>
    </ux-code-block>

    <ux-code-block code="<v-outlined-button class=&quot;danger&quot;>Danger</v-outlined-button>">
      <v-outlined-button class="danger">Danger</v-outlined-button>
    </ux-code-block>

    <ux-code-block code="<v-icon-button>&#10;  <i-material-symbols:settings-rounded />&#10;</v-icon-button>">
      <v-icon-button><i-material-symbols:settings-rounded /></v-icon-button>
      <v-icon-button><i-material-symbols:edit-rounded /></v-icon-button>
      <v-icon-button><i-material-symbols:delete-outline-rounded /></v-icon-button>
      <v-icon-button :loading="true" />
    </ux-code-block>

    <h3>.button-group</h3>
    <ux-code-block code="<div class=&quot;button-group&quot;>&#10;  <button :class=&quot;{ selected: active === 'a' }&quot;>A</button>&#10;  <button :class=&quot;{ selected: active === 'b' }&quot;>B</button>&#10;</div>">
      <div class="button-group">
        <button :class="{ selected: btnGroup === 'a' }" @click="btnGroup = 'a'">Option A</button>
        <button :class="{ selected: btnGroup === 'b' }" @click="btnGroup = 'b'">Option B</button>
        <button :class="{ selected: btnGroup === 'c' }" @click="btnGroup = 'c'">Option C</button>
      </div>
    </ux-code-block>
  </section>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import UxCodeBlock from './UxCodeBlock.vue'

const btnGroup = ref('a')
</script>
